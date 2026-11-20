//! The session state machine of a wireless-display connection.
//!
//! Every phase has a timeout; every failure has a kind; every transition is logged; a failure
//! either schedules a bounded retry (with backoff) or ends the session; whatever was set up
//! (P2P group, sockets, processes) is registered as a cleanup step and undone in reverse order.

use crate::backoff::Backoff;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Idle,
    Discovering,
    DeviceFound,
    Provisioning,
    GroupNegotiation,
    GroupFormed,
    IpConfigured,
    RtspNegotiation,
    StreamReady,
    Streaming,
    Recovering,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Timeout,
    Discovery,
    Provisioning,
    GroupNegotiation,
    Authentication,
    Dhcp,
    ChannelConflict,
    Rtsp,
    Receiver,
    Encoder,
    Capture,
    Network,
    Cancelled,
}

impl FailureKind {
    /// Worth trying again automatically (a timeout or a dropped link), as opposed to a refusal.
    pub fn retryable(self) -> bool {
        !matches!(
            self,
            FailureKind::Authentication | FailureKind::Cancelled | FailureKind::ChannelConflict
        )
    }
}

impl Phase {
    /// How long a phase may last before it counts as a failure (None = no limit).
    pub fn timeout(self) -> Option<Duration> {
        let s = |n| Some(Duration::from_secs(n));
        match self {
            Phase::Discovering => s(30),
            Phase::DeviceFound => s(10),
            Phase::Provisioning => s(30),
            Phase::GroupNegotiation => s(25),
            Phase::GroupFormed => s(20),
            Phase::IpConfigured => s(10),
            Phase::RtspNegotiation => s(15),
            Phase::StreamReady => s(10),
            Phase::Recovering => s(60),
            Phase::Stopping => s(10),
            Phase::Idle | Phase::Streaming | Phase::Failed => None,
        }
    }

    fn next_in_chain(self) -> Option<Phase> {
        use Phase::*;
        Some(match self {
            Idle => Discovering,
            Discovering => DeviceFound,
            DeviceFound => Provisioning,
            Provisioning => GroupNegotiation,
            GroupNegotiation => GroupFormed,
            GroupFormed => IpConfigured,
            IpConfigured => RtspNegotiation,
            RtspNegotiation => StreamReady,
            StreamReady => Streaming,
            _ => return None,
        })
    }

    fn active(self) -> bool {
        !matches!(self, Phase::Idle | Phase::Stopping | Phase::Failed)
    }

    pub fn allows(self, to: Phase) -> bool {
        use Phase::*;
        if self.next_in_chain() == Some(to) {
            return true;
        }
        match (self, to) {
            // Any live phase can stop, fail or go into recovery.
            (f, Stopping | Failed) if f.active() => true,
            (f, Recovering) if f.active() && f != Recovering => true,
            // Recovery starts again from discovery or from the connection step.
            (Recovering, Discovering | Provisioning | RtspNegotiation) => true,
            (Stopping, Idle) | (Failed, Idle) | (Failed, Discovering) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: Phase,
    pub to: Phase,
}

/// One line of the session's transition log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub at: Duration,
    pub from: Phase,
    pub to: Phase,
    pub note: String,
}

/// What to do after a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Try again after this delay (the machine is in `Recovering`).
    Retry(Duration),
    /// Give up (the machine is in `Failed`).
    GiveUp,
}

pub struct Machine {
    phase: Phase,
    started: Instant,
    entered: Instant,
    log: Vec<LogEntry>,
    backoff: Backoff,
    cleanups: Vec<String>,
    failure: Option<(FailureKind, String)>,
}

impl Machine {
    pub fn new(now: Instant, backoff: Backoff) -> Self {
        Self {
            phase: Phase::Idle,
            started: now,
            entered: now,
            log: Vec::new(),
            backoff,
            cleanups: Vec::new(),
            failure: None,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    pub fn failure(&self) -> Option<&(FailureKind, String)> {
        self.failure.as_ref()
    }

    pub fn transition(
        &mut self,
        to: Phase,
        note: impl Into<String>,
        now: Instant,
    ) -> Result<(), IllegalTransition> {
        if !self.phase.allows(to) {
            return Err(IllegalTransition {
                from: self.phase,
                to,
            });
        }
        self.log.push(LogEntry {
            at: now.saturating_duration_since(self.started),
            from: self.phase,
            to,
            note: note.into(),
        });
        self.phase = to;
        self.entered = now;
        if to == Phase::Streaming {
            // A session that got this far starts its retry budget afresh.
            self.backoff.reset();
        }
        Ok(())
    }

    /// Registers something to undo when the session ends (run in reverse order).
    pub fn on_cleanup(&mut self, what: impl Into<String>) {
        self.cleanups.push(what.into());
    }

    /// The cleanup steps, last registered first; the list is emptied.
    pub fn take_cleanups(&mut self) -> Vec<String> {
        let mut c = std::mem::take(&mut self.cleanups);
        c.reverse();
        c
    }

    /// Has the current phase lasted too long? If so it is handled as a timeout failure.
    pub fn check_timeout(&mut self, now: Instant) -> Option<Decision> {
        let limit = self.phase.timeout()?;
        if now.saturating_duration_since(self.entered) <= limit {
            return None;
        }
        let msg = format!("{:?} took longer than {}s", self.phase, limit.as_secs());
        Some(self.fail(FailureKind::Timeout, msg, now))
    }

    /// Records a failure and decides: retry (bounded, backed off) or give up.
    pub fn fail(&mut self, kind: FailureKind, msg: impl Into<String>, now: Instant) -> Decision {
        let msg = msg.into();
        self.failure = Some((kind, msg.clone()));
        if !self.phase.active() {
            return Decision::GiveUp;
        }
        if kind.retryable() {
            if let Some(delay) = self.backoff.next_delay() {
                let _ = self.transition(Phase::Recovering, format!("{kind:?}: {msg}"), now);
                return Decision::Retry(delay);
            }
        }
        let _ = self.transition(Phase::Failed, format!("{kind:?}: {msg}"), now);
        Decision::GiveUp
    }

    /// The teacher stopped the session.
    pub fn stop(&mut self, now: Instant) {
        if self.phase.active() {
            let _ = self.transition(Phase::Stopping, "stopped by the user", now);
        }
        if self.phase == Phase::Stopping {
            let _ = self.transition(Phase::Idle, "stopped", now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(t: Instant) -> Machine {
        Machine::new(
            t,
            Backoff::new(Duration::from_secs(1), Duration::from_secs(8), 3),
        )
    }

    fn walk_to(m: &mut Machine, to: Phase, t: Instant) {
        while m.phase() != to {
            let next = m.phase().next_in_chain().expect("chain");
            m.transition(next, "step", t).unwrap();
        }
    }

    #[test]
    fn happy_path_walks_the_whole_chain_and_logs_each_step() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::Streaming, t);
        assert_eq!(m.log().len(), 9);
        assert_eq!(m.log()[0].from, Phase::Idle);
        assert_eq!(m.log()[8].to, Phase::Streaming);
    }

    #[test]
    fn skipping_a_phase_is_refused() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::Discovering, t);
        let e = m.transition(Phase::Streaming, "jump", t).unwrap_err();
        assert_eq!((e.from, e.to), (Phase::Discovering, Phase::Streaming));
        assert_eq!(m.phase(), Phase::Discovering);
    }

    #[test]
    fn a_phase_that_lasts_too_long_times_out_and_retries() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::Provisioning, t);
        assert!(m.check_timeout(t + Duration::from_secs(29)).is_none());
        let d = m.check_timeout(t + Duration::from_secs(31)).unwrap();
        assert_eq!(d, Decision::Retry(Duration::from_secs(1)));
        assert_eq!(m.phase(), Phase::Recovering);
        assert_eq!(m.failure().unwrap().0, FailureKind::Timeout);
    }

    #[test]
    fn retries_are_bounded_then_the_session_fails() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::RtspNegotiation, t);
        let mut delays = vec![];
        loop {
            match m.fail(FailureKind::Rtsp, "no answer", t) {
                Decision::Retry(d) => {
                    delays.push(d.as_secs());
                    m.transition(Phase::RtspNegotiation, "again", t).unwrap();
                }
                Decision::GiveUp => break,
            }
        }
        assert_eq!(delays, [1, 2, 4]);
        assert_eq!(m.phase(), Phase::Failed);
    }

    #[test]
    fn a_refusal_is_not_retried() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::Provisioning, t);
        assert_eq!(
            m.fail(FailureKind::Authentication, "wrong PIN", t),
            Decision::GiveUp
        );
        assert_eq!(m.phase(), Phase::Failed);
    }

    #[test]
    fn streaming_restores_the_retry_budget() {
        let t = Instant::now();
        let mut m = machine(t);
        walk_to(&mut m, Phase::Streaming, t);
        for _ in 0..5 {
            // Each drop-out after a stable stream starts from the short delay again.
            assert_eq!(
                m.fail(FailureKind::Network, "drop", t),
                Decision::Retry(Duration::from_secs(1))
            );
            m.transition(Phase::Discovering, "re-find", t).unwrap();
            walk_to(&mut m, Phase::Streaming, t);
        }
    }

    #[test]
    fn cleanup_runs_in_reverse_order_once() {
        let t = Instant::now();
        let mut m = machine(t);
        m.on_cleanup("delete the P2P group");
        m.on_cleanup("close the RTSP socket");
        m.on_cleanup("stop the encoder");
        assert_eq!(
            m.take_cleanups(),
            [
                "stop the encoder",
                "close the RTSP socket",
                "delete the P2P group"
            ]
        );
        assert!(m.take_cleanups().is_empty());
    }

    #[test]
    fn stopping_ends_in_idle_from_any_live_phase() {
        let t = Instant::now();
        for p in [Phase::Discovering, Phase::GroupFormed, Phase::Streaming] {
            let mut m = machine(t);
            walk_to(&mut m, p, t);
            m.stop(t);
            assert_eq!(m.phase(), Phase::Idle);
        }
    }

    #[test]
    fn timeouts_exist_for_every_phase_that_waits_on_someone() {
        for p in [
            Phase::Discovering,
            Phase::Provisioning,
            Phase::GroupNegotiation,
            Phase::RtspNegotiation,
            Phase::Recovering,
        ] {
            assert!(p.timeout().is_some(), "{p:?}");
        }
        assert!(Phase::Streaming.timeout().is_none());
    }
}
