//! Starts and stops the lesson windows (`lessonui`): poll, banner, test. Answers come back on
//! the windows' stdout and go to the console through the session's uplink.

use super::*;
use std::io::{BufRead, Write};
use std::process::Child;

struct Procs {
    poll: Option<Child>,
    banner: Option<Child>,
    test: Option<Child>,
}

static PROCS: Mutex<Procs> = Mutex::new(Procs {
    poll: None,
    banner: None,
    test: None,
});

fn stop(c: &mut Option<Child>) {
    if let Some(mut ch) = c.take() {
        let _ = ch.kill();
        let _ = ch.wait();
    }
}

/// Asks a window to finish by itself (the test hands in what is answered).
fn end(c: &mut Option<Child>) {
    if let Some(ch) = c.as_mut() {
        if let Some(stdin) = ch.stdin.as_mut() {
            let _ = writeln!(stdin, "end");
        }
    }
}

/// Spawns `vimora-client <args>` with the JSON on its stdin; each stdout line goes to `on_line`.
fn spawn(
    args: &[&str],
    json: Option<String>,
    on_line: impl Fn(String) + Send + 'static,
) -> Option<Child> {
    let mut cmd = std::process::Command::new(std::env::current_exe().ok()?);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    if let (Some(j), Some(stdin)) = (json, child.stdin.as_mut()) {
        let _ = writeln!(stdin, "{j}");
    }
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                on_line(line);
            }
        });
    }
    Some(child)
}

fn send(uplink: &Arc<Mutex<Option<mpsc::Sender<ClientMsg>>>>, msg: ClientMsg) {
    if let Some(tx) = uplink.lock().unwrap().as_ref() {
        let _ = tx.try_send(msg);
    }
}

pub(super) fn poll(ctx: &Ctx, id: u32, question: String, options: Vec<String>) {
    let mut p = PROCS.lock().unwrap();
    stop(&mut p.poll);
    let json = serde_json::to_string(&crate::lessonui::PollIn { question, options }).ok();
    let uplink = ctx.uplink.clone();
    p.poll = spawn(&["poll"], json, move |line| {
        if let Ok(a) = serde_json::from_str::<crate::lessonui::PollOut>(&line) {
            send(
                &uplink,
                ClientMsg::PollAnswer {
                    id,
                    choice: a.choice,
                    text: a.text,
                },
            );
        }
    });
}

pub(super) fn poll_end() {
    stop(&mut PROCS.lock().unwrap().poll);
}

pub(super) fn banner(goal: String, end_unix: u64) {
    let mut p = PROCS.lock().unwrap();
    stop(&mut p.banner);
    if goal.trim().is_empty() && end_unix == 0 {
        return;
    }
    p.banner = spawn(&["banner", &goal, &end_unix.to_string()], None, |_| {});
}

pub(super) fn test(ctx: &Ctx, def: vimora_core::TestDef) {
    let mut p = PROCS.lock().unwrap();
    stop(&mut p.test);
    let id = def.id;
    let json = serde_json::to_string(&def).ok();
    let uplink = ctx.uplink.clone();
    p.test = spawn(&["test"], json, move |line| {
        if let Ok(answers) = serde_json::from_str::<Vec<vimora_core::TestAnswer>>(&line) {
            send(&uplink, ClientMsg::TestResult { id, answers });
        }
    });
}

pub(super) fn test_end() {
    end(&mut PROCS.lock().unwrap().test);
}
