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

pub(super) fn spawn_overlay(state: &Shared) -> Result<(Child, ChildStdin)> {
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("overlay")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
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
    Ok((child, stdin))
}

pub(super) async fn overlay_task(state: Shared, mut rx: mpsc::Receiver<OverlayReq>) {
    let mut child: Option<(Child, ChildStdin)> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
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
                        OverlayCmd::Code { salt, hash, digits },
                        OverlayCmd::Lock {
                            title: t.clone(),
                            message: m.clone(),
                        },
                    ]
                })
            }
        };
        // Reap an overlay that died (or was killed by the student) so it gets respawned.
        if let Some((c, _)) = &mut child {
            if let Ok(Some(_)) = c.try_wait() {
                warn!("overlay exited; respawning if needed");
                child = None;
            }
        }
        match desired {
            None => {
                if let Some((mut c, stdin)) = child.take() {
                    drop(stdin);
                    let _ = c.kill().await;
                }
            }
            Some(cmds) => {
                let show_mode = matches!(cmds.first(), Some(OverlayCmd::Show));
                let fresh = child.is_none();
                if fresh {
                    match spawn_overlay(&state) {
                        Ok(c) => child = Some(c),
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
