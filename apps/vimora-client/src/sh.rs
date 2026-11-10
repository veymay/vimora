//! Small helpers for running system tools quietly (no console window on Windows).

use anyhow::{Context, Result};
use std::process::{Command, Output};

pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Runs `prog args…`, returns (exit code, stdout+stderr).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn run_code(prog: &str, args: &[&str]) -> Result<(Option<i32>, String)> {
    let out: Output = quiet(Command::new(prog).args(args))
        .output()
        .with_context(|| format!("{prog} topilmadi"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code(), text))
}

/// Runs `prog args…`, returns (success, stdout+stderr).
pub fn run(prog: &str, args: &[&str]) -> Result<(bool, String)> {
    let out: Output = quiet(Command::new(prog).args(args))
        .output()
        .with_context(|| format!("{prog} topilmadi"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    Ok((out.status.success(), text))
}

/// Like [`run`] but turns a non-zero exit into an error carrying the output's last lines.
pub fn run_ok(prog: &str, args: &[&str]) -> Result<String> {
    let (ok, text) = run(prog, args)?;
    if ok {
        Ok(text)
    } else {
        anyhow::bail!("{prog} xato: {}", tail(&text, 4))
    }
}

/// Runs a shell snippet (`sh -c` / PowerShell).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn script(code: &str) -> Result<(bool, String)> {
    #[cfg(windows)]
    {
        run(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                code,
            ],
        )
    }
    #[cfg(not(windows))]
    {
        run("sh", &["-c", code])
    }
}

pub fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines[lines.len().saturating_sub(n)..].join(" · ")
}

pub fn restart_soon() -> Result<()> {
    #[cfg(windows)]
    run_ok(
        "shutdown",
        &[
            "/r",
            "/t",
            "10",
            "/c",
            "Vimora: kompyuter qayta yuklanmoqda",
        ],
    )?;
    #[cfg(not(windows))]
    {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = run("systemctl", &["reboot"]);
        });
    }
    Ok(())
}

/// Files and registry values a privileged operation changed, so Shadow mode can commit them.
#[derive(Debug, Default, Clone)]
pub struct Touched {
    pub files: Vec<std::path::PathBuf>,
    /// (key, value name)
    pub registry: Vec<(String, String)>,
}
