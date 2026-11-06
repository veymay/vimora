//! Keeps the student's chat panel (`vimora-client chat`) alive while a chat is open and
//! forwards messages in both directions.

use super::*;
use crate::chat::ChatIn;
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin};

pub(super) struct ChatProc {
    child: Child,
    stdin: ChildStdin,
}

pub(super) type SharedChat = Arc<Mutex<Option<ChatProc>>>;

fn frame(msg: &ChatIn) -> Result<Vec<u8>> {
    let body = bincode::serialize(msg)?;
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend(body);
    Ok(out)
}

fn spawn(ctx: &Ctx) -> Result<ChatProc> {
    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.arg("chat").stdin(Stdio::piped()).stdout(Stdio::piped());
    // Wayland doesn't let apps position their windows; run the panel through XWayland so it
    // can dock to the right screen edge (no-op on Windows and plain X11).
    #[cfg(all(unix, not(target_os = "macos")))]
    if std::env::var_os("DISPLAY").is_some() {
        cmd.env_remove("WAYLAND_DISPLAY");
    }
    let mut child = cmd.spawn()?;
    let stdin = child.stdin.take().context("no stdin")?;
    let stdout = child.stdout.take().context("no stdout")?;
    let uplink = ctx.uplink.clone();
    // Student replies → console.
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let Ok(text) = serde_json::from_str::<String>(&line) else {
                continue;
            };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            if let Some(tx) = uplink.lock().unwrap().as_ref() {
                let _ = tx.try_send(ClientMsg::Chat { text });
            }
        }
    });
    Ok(ChatProc { child, stdin })
}

/// Shows a teacher message, opening the panel if needed (also after the teacher ended a chat).
pub(super) fn deliver(ctx: &Ctx, text: &str) -> Result<()> {
    let msg = frame(&ChatIn::Teacher {
        text: text.to_string(),
        time: crate::chat::now_hhmm(),
    })?;
    let mut slot = ctx.chat.lock().unwrap();
    // Respawn if it never ran or the student killed it.
    let alive = slot
        .as_mut()
        .is_some_and(|c| matches!(c.child.try_wait(), Ok(None)));
    if !alive {
        *slot = Some(spawn(ctx)?);
    }
    let proc_ = slot.as_mut().expect("chat process");
    if proc_
        .stdin
        .write_all(&msg)
        .and_then(|_| proc_.stdin.flush())
        .is_err()
    {
        // Pipe broke between the check and the write: one fresh attempt.
        let mut fresh = spawn(ctx)?;
        fresh.stdin.write_all(&msg)?;
        fresh.stdin.flush()?;
        *slot = Some(fresh);
    }
    Ok(())
}

/// The teacher ended the chat: close the panel; it stays closed until the next message.
pub(super) fn end(ctx: &Ctx) {
    if let Some(mut c) = ctx.chat.lock().unwrap().take() {
        if let Ok(f) = frame(&ChatIn::End) {
            let _ = c.stdin.write_all(&f).and_then(|_| c.stdin.flush());
        }
        drop(c.stdin);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            let _ = c.child.kill();
            let _ = c.child.wait();
        });
    }
}
