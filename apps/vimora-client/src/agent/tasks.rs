//! Long-running background work: screen capture, app blocking, keyword watcher, service status.

use super::*;

pub(super) fn capture_loop(
    cfg: Arc<Mutex<StreamConfig>>,
    stop: Arc<AtomicBool>,
    tx: mpsc::Sender<ClientMsg>,
) {
    let mut cap = match capture::Capturer::new() {
        Ok(c) => c,
        Err(e) => {
            warn!("screen capture unavailable: {e:#}");
            return;
        }
    };
    let mut last: Vec<u8> = Vec::new();
    let mut warned = false;
    let mut last_sent = Instant::now() - Duration::from_secs(60);
    while !stop.load(Ordering::Relaxed) {
        let c = *cfg.lock().unwrap();
        if c.interval_ms == 0 {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        }
        let t0 = Instant::now();
        match cap
            .capture()
            .and_then(|f| capture::encode_jpeg(&f, c.max_width, c.quality))
        {
            Ok((width, height, jpeg)) => {
                // Skip identical frames, but refresh at least every 5 s.
                if jpeg != last || last_sent.elapsed() > Duration::from_secs(5) {
                    last_sent = Instant::now();
                    last = jpeg.clone();
                    let _ = tx.try_send(ClientMsg::Frame {
                        width,
                        height,
                        jpeg,
                    });
                }
            }
            Err(e) => {
                if !warned {
                    warn!("capture: {e:#}");
                    warned = true;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        let spent = t0.elapsed();
        let wait = Duration::from_millis(c.interval_ms as u64).saturating_sub(spent);
        // Sleep in small steps so config changes (thumbnail -> live) take effect quickly.
        let until = Instant::now() + wait;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            if cfg.lock().unwrap().interval_ms != c.interval_ms {
                break;
            }
            std::thread::sleep(
                Duration::from_millis(20).min(until.saturating_duration_since(Instant::now())),
            );
        }
    }
}

pub(super) async fn blocker_task(state: Shared) {
    let mut blocker = system::AppBlocker::new();
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let list = state.lock().unwrap().apps.clone();
        if !list.is_empty() {
            let (killed, b) = tokio::task::spawn_blocking(move || {
                let k = blocker.enforce(&list);
                (k, blocker)
            })
            .await
            .unwrap();
            blocker = b;
            for k in killed {
                info!("blocked app terminated: {k}");
            }
        }
    }
}

pub(super) async fn auto_unlock_task(state: Shared, ov_tx: mpsc::Sender<OverlayReq>, secs: u64) {
    if secs == 0 {
        return;
    }
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let unlock = {
            let mut s = state.lock().unwrap();
            let expired = !s.connected
                && s.disconnected_at
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(secs));
            if expired && s.lock.is_some() {
                s.lock = None;
                true
            } else {
                false
            }
        };
        if unlock {
            info!("console gone for {secs}s — unlocking");
            let _ = ov_tx.send(OverlayReq::Sync).await;
        }
    }
}

pub(super) async fn net_enforce_task(engine: Arc<Engine>) {
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let e = engine.clone();
        let _ = tokio::task::spawn_blocking(move || e.enforce_net()).await;
    }
}

/// Mirrors the service's state (freeze, updates, background jobs) into our status.
pub(super) async fn service_status_task(ctx: Ctx) {
    let mut last = ClientStatus::default();
    let mut last_apps_rev = 0u64;
    let mut last_changes_rev = 0u64;
    let mut last_uplink: Option<mpsc::Sender<ClientMsg>> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let st = match service::call(IpcReq::Status).await {
            Ok(IpcResp::Status(st)) => Some(st),
            _ => None,
        };
        let (st, service) = match st {
            Some(st) => (st, true),
            None => {
                let e = ctx.engine.clone();
                (
                    tokio::task::spawn_blocking(move || e.status())
                        .await
                        .unwrap_or_default(),
                    false,
                )
            }
        };
        let status = {
            let mut s = ctx.state.lock().unwrap();
            s.service = service;
            s.freeze = st.freeze;
            s.shield_usage = st.shield_usage;
            s.shield_note = st.shield_note.clone();
            s.os_updates = st.os_updates;
            s.app_updates = st.app_updates;
            s.busy = st.busy.clone();
            s.queued = st.queued;
            s.persistent_folders = st.persistent_folders.clone();
            s.status()
        };
        let uplink = ctx.uplink.lock().unwrap().clone();
        if let Some(tx) = uplink {
            // New console session: everything must be sent again.
            if !last_uplink.as_ref().is_some_and(|l| l.same_channel(&tx)) {
                last = ClientStatus::default();
                last_apps_rev = 0;
                last_changes_rev = 0;
                last_uplink = Some(tx.clone());
            }
            if let Some((rev, list)) = st.apps {
                if rev != last_apps_rev {
                    last_apps_rev = rev;
                    let _ = tx.send(ClientMsg::Apps(list)).await;
                }
            }
            if let Some((rev, list)) = st.changes {
                if rev != last_changes_rev {
                    last_changes_rev = rev;
                    let _ = tx.send(ClientMsg::Changes(list)).await;
                }
            }
            for (ok, message) in st.events {
                let _ = tx.send(ClientMsg::CommandResult { ok, message }).await;
            }
            if status != last {
                let _ = tx.send(ClientMsg::Status(status.clone())).await;
                last = status;
            }
        }
    }
}

/// Watches the focused browser window title for blocked keywords.
pub(super) fn keyword_watch(ctx: Ctx) {
    let mut watcher: Option<netcontrol::FocusWatcher> = None;
    let mut injector: Option<input::Injector> = None;
    let mut warned = false;
    loop {
        std::thread::sleep(Duration::from_millis(700));
        let keywords = ctx.state.lock().unwrap().net.keywords.clone();
        if keywords.is_empty() {
            continue;
        }
        if watcher.is_none() {
            match netcontrol::FocusWatcher::new() {
                Ok(w) => watcher = Some(w),
                Err(e) => {
                    if !warned {
                        warn!("keyword watcher unavailable: {e:#}");
                        warned = true;
                    }
                    std::thread::sleep(Duration::from_secs(10));
                    continue;
                }
            }
        }
        let Some(f) = watcher.as_mut().and_then(|w| w.focused()) else {
            continue;
        };
        if !f.is_browser() {
            continue;
        }
        let title = netcontrol::normalize(&f.title);
        let Some(hit) = keywords.iter().find(|k| title.contains(k.as_str())) else {
            continue;
        };

        info!("blocked keyword \"{hit}\" in \"{}\" ({})", f.title, f.app);
        if injector.is_none() {
            injector = input::Injector::new().ok();
        }
        if let Some(i) = &mut injector {
            let _ = i.close_tab();
        }
        let _ = spawn_self(&[
            "popup",
            "Taqiqlangan qidiruv",
            &format!("\u{201c}{hit}\u{201d} bo'yicha qidirish taqiqlangan.\nSahifa yopildi va o'qituvchiga xabar berildi."),
        ]);
        if let Some(tx) = ctx.uplink.lock().unwrap().as_ref() {
            let _ = tx.try_send(ClientMsg::Alert {
                text: format!("taqiqlangan so'z \"{hit}\" — {}", f.title),
            });
        }
        std::thread::sleep(Duration::from_millis(2500));
    }
}
