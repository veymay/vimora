//! Supervises the fullscreen overlay process (lock screen / presentation) and keeps it alive.

use super::*;

pub(super) enum OverlayReq {
    Sync,
    Frame(Vec<u8>),
}

pub(super) async fn write_overlay(stdin: &mut ChildStdin, cmd: &OverlayCmd) -> Result<()> {
    let body = bincode::serialize(cmd)?;
    stdin.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stdin.write_all(&body).await?;
    stdin.flush().await?;
    Ok(())
}

/// How the overlay is started, tried in this order until one stays up: (renderer, own desktop).
/// Always the normal desktop with the backend the startup check found, then the other one.
/// (A separate lock desktop was tried in 0.3.11–0.3.15: on many PCs OpenGL doesn't work there
/// and Direct3D shows only black, so it is not used; the keyboard hooks, the guard and the
/// Windows policies keep the student on the lock screen instead.) A plan that worked is kept.
fn plans() -> Vec<(&'static str, bool)> {
    let usual = match std::env::var("VIMORA_RENDERER").as_deref() {
        Ok("wgpu") => "wgpu",
        _ => "glow",
    };
    let other = if usual == "wgpu" { "glow" } else { "wgpu" };
    vec![(usual, false), (other, false)]
}

/// Index into [`plans`] of the way the overlay starts now (`usize::MAX` = not loaded yet).
static PLAN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);

fn plan_file() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("overlay-plan-2"))
}

/// The plan to use: the one that worked last time on this PC, else the first.
fn current_plan() -> usize {
    use std::sync::atomic::Ordering::Relaxed;
    let p = PLAN.load(Relaxed);
    if p != usize::MAX {
        return p;
    }
    let saved = plan_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|p| *p < plans().len())
        .unwrap_or(0);
    PLAN.store(saved, Relaxed);
    saved
}

/// The overlay stayed up: keep this way of starting it for the next time.
fn remember_plan() {
    if let Some(f) = plan_file() {
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
        let _ = std::fs::write(f, current_plan().to_string());
    }
}

pub(super) fn spawn_overlay(state: &Shared) -> Result<(Child, ChildStdin)> {
    let plans = plans();
    let (renderer, own_desktop) = plans[current_plan().min(plans.len() - 1)];
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("overlay")
        .env("VIMORA_RENDERER", renderer)
        .env("VIMORA_LOCK_DESKTOP", if own_desktop { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdin = child.stdin.take().context("no stdin")?;
    // The overlay prints "UNLOCK" when the student typed the right code.
    if let Some(out) = child.stdout.take() {
        let state = state.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim() == "UNLOCK" {
                    info!("lock screen opened with the unlock code");
                    {
                        let mut s = state.lock().unwrap();
                        s.lock = None;
                        s.unlock = None;
                    }
                    save_lock(&state);
                }
            }
        });
    }
    // Keep the last thing it complained about: if it dies at once, the teacher is told why.
    if let Some(err) = child.stderr.take() {
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if !line.is_empty() {
                    *LAST_ERROR.lock().unwrap() = line.chars().take(160).collect();
                }
            }
        });
    }
    Ok((child, stdin))
}

static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

pub(super) async fn overlay_task(state: Shared, mut rx: mpsc::Receiver<OverlayReq>) {
    let mut child: Option<(Child, ChildStdin)> = None;
    let mut born = Instant::now();
    let mut quick_deaths = 0u32;
    // The current overlay lived long enough for its start plan to be remembered.
    let mut plan_saved = false;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    // System lockdown (Windows policies via the service) while the lock screen is up;
    // re-asserted every 30 s in case the service restarted.
    let mut kiosk: Option<(bool, Instant)> = None;
    loop {
        let req = tokio::select! {
            r = rx.recv() => match r { Some(r) => Some(r), None => break },
            _ = tick.tick() => None,
        };
        let desired: Option<Vec<OverlayCmd>> = {
            let s = state.lock().unwrap();
            if s.show {
                Some(vec![OverlayCmd::Show])
            } else {
                s.lock.as_ref().map(|(t, m)| {
                    let (salt, hash, digits) = s.unlock.clone().unwrap_or_default();
                    vec![
                        OverlayCmd::Style {
                            icon: s.lock_icon.clone(),
                        },
                        OverlayCmd::Code { salt, hash, digits },
                        OverlayCmd::Lock {
                            title: t.clone(),
                            message: m.clone(),
                        },
                    ]
                })
            }
        };
        let locked = {
            let s = state.lock().unwrap();
            s.lock.is_some() && !s.show
        };
        let due = match kiosk {
            None => locked,
            Some((on, at)) => on != locked || (locked && at.elapsed() > Duration::from_secs(30)),
        };
        if due {
            kiosk = Some((locked, Instant::now()));
            tokio::spawn(async move {
                match service::call(IpcReq::Kiosk(locked)).await {
                    Ok(IpcResp::Done { ok: false, message }) => warn!("kiosk: {message}"),
                    Err(e) => warn!("kiosk: no service ({e:#})"),
                    _ => {}
                }
            });
        }
        // Reap an overlay that died (or was killed by the student) so it gets respawned.
        if let Some((c, _)) = &mut child {
            if let Ok(Some(_)) = c.try_wait() {
                warn!("overlay exited; respawning if needed");
                child = None;
                // Never leave the student on the (empty) lock desktop.
                crate::desktop::restore();
                // Died right after starting while it is still wanted: the window could not
                // open. Quietly try the next way to start it; the teacher hears about it only
                // when none works.
                let wanted = {
                    let s = state.lock().unwrap();
                    s.show || s.lock.is_some()
                };
                if wanted && born.elapsed() < Duration::from_secs(4) {
                    use std::sync::atomic::Ordering::Relaxed;
                    let last = plans().len() - 1;
                    let plan = current_plan();
                    if plan < last {
                        PLAN.store(plan + 1, Relaxed);
                        warn!(
                            "overlay could not start ({}); trying {:?}",
                            LAST_ERROR.lock().unwrap(),
                            plans()[plan + 1]
                        );
                    } else {
                        quick_deaths += 1;
                        if quick_deaths == 1 {
                            let why = LAST_ERROR.lock().unwrap().clone();
                            report_fault(format!(
                                "Qulf ekrani bu kompyuterda ochilmayapti: grafik drayverni yangilang ({why})"
                            ));
                        }
                        // Start the round again later (a driver may come up after login).
                        if quick_deaths >= 10 {
                            PLAN.store(0, Relaxed);
                            quick_deaths = 0;
                        }
                    }
                } else {
                    quick_deaths = 0;
                }
            } else if born.elapsed() > Duration::from_secs(5) && !plan_saved {
                plan_saved = true;
                remember_plan();
            }
        }
        match desired {
            None => {
                if let Some((mut c, mut stdin)) = child.take() {
                    // Let it fade out (≈0.35 s), then make sure it is gone.
                    let _ = write_overlay(&mut stdin, &OverlayCmd::Bye).await;
                    let gone = tokio::time::timeout(Duration::from_millis(600), c.wait()).await;
                    drop(stdin);
                    if gone.is_err() {
                        let _ = c.kill().await;
                    }
                    crate::desktop::restore();
                }
            }
            Some(cmds) => {
                let show_mode = matches!(cmds.first(), Some(OverlayCmd::Show));
                let fresh = child.is_none();
                if fresh {
                    match spawn_overlay(&state) {
                        Ok(c) => {
                            child = Some(c);
                            born = Instant::now();
                            plan_saved = false;
                        }
                        Err(e) => {
                            warn!("cannot start overlay: {e:#}");
                            continue;
                        }
                    }
                }
                let Some((_, stdin)) = &mut child else {
                    continue;
                };
                let mut ok = true;
                if fresh || matches!(req, Some(OverlayReq::Sync)) {
                    for cmd in &cmds {
                        ok &= write_overlay(stdin, cmd).await.is_ok();
                    }
                }
                if let (Some(OverlayReq::Frame(jpeg)), true) = (req, show_mode) {
                    ok &= write_overlay(stdin, &OverlayCmd::Frame { jpeg })
                        .await
                        .is_ok();
                }
                if !ok {
                    child = None;
                }
            }
        }
    }
}
