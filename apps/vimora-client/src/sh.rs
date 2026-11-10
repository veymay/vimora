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

/// Program output as text: some Windows tools (uwfmgr, …) write UTF-16 when their output
/// goes to a pipe; the rest is UTF-8 (or close enough).
pub fn decode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    if bytes.len() >= 4 && zeros * 3 > bytes.len() / 2 {
        let body = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units).replace('\0', "");
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `prog args…`, returns (exit code, stdout+stderr).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn run_code(prog: &str, args: &[&str]) -> Result<(Option<i32>, String)> {
    let out: Output = quiet(Command::new(prog).args(args))
        .output()
        .with_context(|| format!("{prog} topilmadi"))?;
    let mut text = decode(&out.stdout);
    text.push('\n');
    text.push_str(&decode(&out.stderr));
    Ok((out.status.code(), text))
}

/// Runs `prog args…`, returns (success, stdout+stderr).
pub fn run(prog: &str, args: &[&str]) -> Result<(bool, String)> {
    let out: Output = quiet(Command::new(prog).args(args))
        .output()
        .with_context(|| format!("{prog} topilmadi"))?;
    let mut text = decode(&out.stdout);
    let err = decode(&out.stderr);
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

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_utf16_output() {
        let wide: Vec<u8> = "Filter state: ON\r\n"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(super::decode(&wide), "Filter state: ON\r\n");
        assert_eq!(super::decode(b"plain"), "plain");
    }
}
