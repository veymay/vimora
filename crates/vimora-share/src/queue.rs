//! The queue between capture and encoder: bounded, newest frame wins.
//!
//! In live screen sharing a stale frame is worse than a missing one, so when the encoder is
//! slower than capture the oldest frames are dropped (and counted), memory never grows, and the
//! capture side never blocks.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A captured frame with the moment it was taken.
#[derive(Debug)]
pub struct Stamped<T> {
    pub captured: Instant,
    pub data: T,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueStats {
    pub pushed: u64,
    pub popped: u64,
    /// Frames thrown away because the queue was full or they were too old.
    pub dropped: u64,
    pub max_depth: usize,
    /// Age of the last frame handed out.
    pub last_age: Duration,
    pub max_age: Duration,
}

pub struct LatestQueue<T> {
    items: VecDeque<Stamped<T>>,
    capacity: usize,
    /// Frames older than this are never handed out (0 = no limit).
    max_age: Duration,
    stats: QueueStats,
}

impl<T> LatestQueue<T> {
    /// `capacity` ≥ 1 (clamped); a small number (2) keeps latency low.
    pub fn new(capacity: usize, max_age: Duration) -> Self {
        Self {
            items: VecDeque::new(),
            capacity: capacity.max(1),
            max_age,
            stats: QueueStats::default(),
        }
    }

    /// Adds a frame; when full, the oldest one is dropped to make room. Never blocks.
    pub fn push(&mut self, data: T, captured: Instant) {
        self.stats.pushed += 1;
        while self.items.len() >= self.capacity {
            self.items.pop_front();
            self.stats.dropped += 1;
        }
        self.items.push_back(Stamped { captured, data });
        self.stats.max_depth = self.stats.max_depth.max(self.items.len());
    }

    /// The oldest frame still fresh enough; frames that waited too long are dropped on the way.
    pub fn pop(&mut self, now: Instant) -> Option<Stamped<T>> {
        while let Some(f) = self.items.pop_front() {
            let age = now.saturating_duration_since(f.captured);
            if !self.max_age.is_zero() && age > self.max_age {
                self.stats.dropped += 1;
                continue;
            }
            self.stats.popped += 1;
            self.stats.last_age = age;
            self.stats.max_age = self.stats.max_age.max(age);
            return Some(f);
        }
        None
    }

    /// Takes only the newest frame and drops the rest (an encoder that fell behind catches up).
    pub fn pop_newest(&mut self, now: Instant) -> Option<Stamped<T>> {
        while self.items.len() > 1 {
            self.items.pop_front();
            self.stats.dropped += 1;
        }
        self.pop(now)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_grows_past_its_capacity_and_counts_drops() {
        let mut q = LatestQueue::new(2, Duration::ZERO);
        let t = Instant::now();
        for i in 0..1000 {
            q.push(i, t);
            assert!(q.len() <= 2);
        }
        assert_eq!(q.stats().dropped, 998);
        assert_eq!(q.stats().max_depth, 2);
        // The two newest remain, oldest first.
        assert_eq!(q.pop(t).unwrap().data, 998);
        assert_eq!(q.pop(t).unwrap().data, 999);
        assert!(q.pop(t).is_none());
    }

    #[test]
    fn stale_frames_are_dropped_not_delivered() {
        let mut q = LatestQueue::new(8, Duration::from_millis(200));
        let t0 = Instant::now();
        q.push("old", t0);
        q.push("fresh", t0 + Duration::from_millis(900));
        let got = q.pop(t0 + Duration::from_secs(1)).unwrap();
        assert_eq!(got.data, "fresh");
        assert_eq!(q.stats().dropped, 1);
        assert!(q.stats().last_age <= Duration::from_millis(200));
    }

    #[test]
    fn pop_newest_catches_up_in_one_step() {
        let mut q = LatestQueue::new(4, Duration::ZERO);
        let t = Instant::now();
        for i in 0..4 {
            q.push(i, t);
        }
        assert_eq!(q.pop_newest(t).unwrap().data, 3);
        assert_eq!(q.stats().dropped, 3);
        assert!(q.is_empty());
    }

    #[test]
    fn zero_capacity_is_treated_as_one() {
        let mut q = LatestQueue::new(0, Duration::ZERO);
        let t = Instant::now();
        q.push(1, t);
        q.push(2, t);
        assert_eq!(q.pop(t).unwrap().data, 2);
    }
}
