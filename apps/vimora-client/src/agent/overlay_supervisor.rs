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

pub(super) fn spawn_overlay() -> Result<(Child, ChildStdin)> {
    let mut child = tokio::process::Command::new(std::env::current_exe()?)
        .arg("overlay")
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdin = child.stdin.take().context("no stdin")?;
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
        let desired = {
            let s = state.lock().unwrap();
            if s.show {
                Some(OverlayCmd::Show)
            } else {
                s.lock.as_ref().map(|(t, m)| OverlayCmd::Lock {
                    title: t.clone(),
                    message: m.clone(),
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
            Some(cmd) => {
                let fresh = child.is_none();
                if fresh {
                    match spawn_overlay() {
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
                    ok &= write_overlay(stdin, &cmd).await.is_ok();
                }
                if let (Some(OverlayReq::Frame(jpeg)), OverlayCmd::Show) = (req, &cmd) {
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
