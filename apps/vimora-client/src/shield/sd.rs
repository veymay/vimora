//! Windows backend: **Shadow Defender**, driven through its `CmdTool.exe`.
//!
//! Vimora does not ship or install Shadow Defender (it is a commercial program with its own
//! license); it only finds an installed copy, wherever it is, and sends it the commands that
//! `CmdTool.txt` documents: `/pwd`, `/list`, `/enter`, `/exit`, `/commit`, `/now`.
//!
//! `CmdTool.exe` is a GUI-subsystem program and may print nothing, so the state is read from
//! `/list` when it answers and otherwise from a small state file Vimora keeps itself.

use super::{Backend, Probe};
use crate::sh;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use vimora_core::FreezeState;

pub struct ShadowDefender;

static FOUND: Mutex<Option<PathBuf>> = Mutex::new(None);

const TOOL: &str = "CmdTool.exe";
const STATE: &str = "sd.state";
const PASSWORD: &str = "sd.pwd";

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
        .join("Vimora")
}

fn system_drive_letter() -> char {
    std::env::var("SystemDrive")
        .ok()
        .and_then(|d| d.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('C')
}

/// Install folders mentioned in `reg query … /s /f "Shadow Defender"` output.
pub fn dirs_from_reg_output(out: &str) -> Vec<PathBuf> {
    let mut v = vec![];
    for line in out.lines() {
        let Some(i) = line.find("REG_") else {
            continue;
        };
        // "REG_SZ    C:\Program Files\Shadow Defender\Defender.exe,0"
        let value = line[i..]
            .split_once(char::is_whitespace)
            .map_or("", |x| x.1);
        let value = value.trim().trim_matches('"');
        if !value.to_lowercase().contains("shadow defender") {
            continue;
        }
        let lower = value.to_lowercase();
        // Plain string work: the paths are Windows paths whichever OS runs the tests.
        let mut dir = match lower.find(".exe") {
            Some(e) => {
                let exe = &value[..e + 4];
                exe.rfind('\\').map_or("", |i| &exe[..i])
            }
            None => value,
        };
        dir = dir.trim_matches('"').trim_end_matches('\\');
        let d = PathBuf::from(dir);
        if !dir.is_empty() && !v.contains(&d) {
            v.push(d);
        }
    }
    v
}

/// Finds an installed `CmdTool.exe`, wherever Shadow Defender lives.
fn find_tool() -> Option<PathBuf> {
    if let Some(p) = FOUND.lock().unwrap().clone().filter(|p| p.exists()) {
        return Some(p);
    }
    let mut dirs: Vec<PathBuf> = vec![];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(pf) = std::env::var_os(var) {
            dirs.push(Path::new(&pf).join("Shadow Defender"));
        }
    }
    for l in 'C'..='Z' {
        for pf in ["Program Files", "Program Files (x86)"] {
            dirs.push(PathBuf::from(format!("{l}:\\{pf}\\Shadow Defender")));
        }
    }
    let mut found = dirs.into_iter().find(|d| d.join(TOOL).exists());
    if found.is_none() && cfg!(windows) {
        for root in [
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            if let Ok((_, out)) = sh::run("reg", &["query", root, "/s", "/f", "Shadow Defender"]) {
                found = dirs_from_reg_output(&out)
                    .into_iter()
                    .find(|d| d.join(TOOL).exists());
                if found.is_some() {
                    break;
                }
            }
        }
    }
    let exe = found.map(|d| d.join(TOOL));
    *FOUND.lock().unwrap() = exe.clone();
    exe
}

/// Drive letters found in `/list` output (`C:` or a bare letter on its own).
pub fn parse_list(out: &str) -> Vec<char> {
    let mut v = vec![];
    for tok in out.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let t = tok.trim_matches(|c: char| matches!(c, '\\' | '/' | '(' | ')' | '[' | ']'));
        let letter = match t.len() {
            1 => t.chars().next(),
            2 if t.ends_with(':') => t.chars().next(),
            _ => None,
        };
        if let Some(c) = letter.filter(char::is_ascii_alphabetic) {
            let c = c.to_ascii_uppercase();
            if !v.contains(&c) {
                v.push(c);
            }
        }
    }
    v
}

fn password() -> String {
    std::fs::read_to_string(data_dir().join(PASSWORD))
        .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
        .unwrap_or_default()
}

fn read_state() -> String {
    std::fs::read_to_string(data_dir().join(STATE))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn write_state(s: &str) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(STATE), s);
}

/// Quotes a CmdTool argument the way its help shows them: `/commit:"C:\Test dir"`.
fn arg_with_value(name: &str, value: &str) -> String {
    if value.contains(' ') || name == "/commit" {
        format!("{name}:\"{value}\"")
    } else {
        format!("{name}:{value}")
    }
}

/// Runs CmdTool and returns (exit code, printed text). It is a GUI-subsystem program: it must be
/// started with its output redirected to files and waited for, otherwise nothing is captured.
#[cfg(windows)]
fn run_tool(tool: &Path, args: &[String]) -> Result<(i32, String)> {
    let ps_quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let list = args
        .iter()
        .map(|a| ps_quote(a))
        .collect::<Vec<_>>()
        .join(",");
    let code = format!(
        "$o=[IO.Path]::GetTempFileName(); $e=[IO.Path]::GetTempFileName(); \
         try {{ $p=Start-Process -FilePath {tool} -ArgumentList @({list}) -Wait -PassThru \
           -WindowStyle Hidden -RedirectStandardOutput $o -RedirectStandardError $e; \
           'EXIT:' + $p.ExitCode; Get-Content -Raw $o; Get-Content -Raw $e }} \
         finally {{ Remove-Item $o,$e -ErrorAction SilentlyContinue }}",
        tool = ps_quote(&tool.to_string_lossy()),
    );
    let (_, out) = sh::script(&code)?;
    Ok(split_exit(&out))
}

#[cfg(not(windows))]
fn run_tool(tool: &Path, args: &[String]) -> Result<(i32, String)> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, out) = sh::run(&tool.to_string_lossy(), &strs)?;
    Ok((if ok { 0 } else { 1 }, out))
}

/// Splits the `EXIT:<code>` marker line from the rest of the output.
pub fn split_exit(out: &str) -> (i32, String) {
    let mut code = -1;
    let mut rest = vec![];
    for l in out.lines() {
        match l.trim().strip_prefix("EXIT:") {
            Some(c) if code == -1 => code = c.trim().parse().unwrap_or(-1),
            _ => rest.push(l),
        }
    }
    (code, rest.join("\n").trim().to_string())
}

/// Runs CmdTool with the stored password. Returns what it printed (often nothing).
fn cmd(args: &[&str]) -> Result<String> {
    let Some(tool) = find_tool() else {
        bail!("Shadow Defender o'rnatilmagan (CmdTool.exe topilmadi)");
    };
    let mut all = vec![arg_with_value("/pwd", &password())];
    all.extend(args.iter().map(|a| {
        // "/enter:C" → name and value split at the first colon.
        match a.split_once(':') {
            Some((n, v)) => arg_with_value(n, v),
            None => a.to_string(),
        }
    }));
    let (code, out) = run_tool(&tool, &all)?;
    if code != 0 {
        bail!(
            "Shadow Defender buyrug'i bajarilmadi ({}, kod {code}) — parol to'g'rimi? {}",
            args.first().copied().unwrap_or(""),
            sh::tail(&out, 2)
        );
    }
    Ok(out)
}

impl Backend for ShadowDefender {
    fn probe(&self) -> Probe {
        if find_tool().is_none() {
            return Probe {
                state: FreezeState::Unsupported,
                usage: None,
                note: Some(
                    "Shadow Defender o'rnatilmagan — uni shu kompyuterga o'rnating (shadowdefender.com)"
                        .into(),
                ),
            };
        }
        let saved = read_state();
        let listed = cmd(&["/list"]);
        let raw = listed
            .as_ref()
            .map(|o| o.trim().to_string())
            .unwrap_or_default();
        let err = listed.as_ref().err().map(|e| format!("{e:#}"));
        let letters = listed.map(|o| parse_list(&o)).unwrap_or_default();
        let sys = system_drive_letter();
        // /list answered: trust it. Otherwise fall back to what Vimora last asked for.
        let on_now = if letters.is_empty() {
            matches!(saved.as_str(), "on" | "off-pending")
        } else {
            letters.contains(&sys)
        };
        let state = match (on_now, saved.as_str()) {
            (true, "off-pending") => FreezeState::ThawPending,
            (true, _) => FreezeState::Frozen,
            (false, "on-pending") => FreezeState::FreezePending,
            (false, _) => FreezeState::Thawed,
        };
        // Say what the program answered, so a wrong guess is visible in the console.
        let note = match (&err, raw.is_empty()) {
            (Some(e), _) => Some(e.clone()),
            (None, true) => Some("Shadow Defender javob bermadi (holat taxminiy)".into()),
            (None, false) => Some(format!("Shadow Defender: {}", sh::tail(&raw, 3))),
        };
        Probe {
            state,
            usage: None,
            note,
        }
    }

    fn enable(&self, _persistent: &[String]) -> Result<String> {
        let drive = format!("/enter:{}", system_drive_letter());
        match cmd(&[&drive, "/now"]) {
            Ok(_) => {
                write_state("on");
                Ok("Shadow Defender yoqildi — himoya darhol ishlaydi".into())
            }
            Err(_) => {
                cmd(&[&drive])?;
                write_state("on-pending");
                Ok("Shadow Defender qayta yuklangandan keyin yoqiladi".into())
            }
        }
    }

    fn disable(&self) -> Result<String> {
        cmd(&[&format!("/exit:{}", system_drive_letter())])?;
        write_state("off-pending");
        Ok("Shadow Defender qayta yuklangandan keyin o'chadi".into())
    }

    fn commit_file(&self, path: &Path) -> Result<()> {
        cmd(&[&format!("/commit:{}", path.to_string_lossy())])?;
        Ok(())
    }

    fn set_persistent(&self, _new: &[String], _old: &[String]) -> Result<String> {
        bail!("Shadow Defender'da istisna papkalar uning o'z oynasida (Exclusion List) sozlanadi")
    }

    fn mount_persistent(&self, _folders: &[String]) {
        // Service start = the PC has just booted: a pending exit/enter has been applied.
        match read_state().as_str() {
            "off-pending" => write_state("off"),
            "on-pending" => write_state("on"),
            _ => {}
        }
    }

    fn set_password(&self, pw: &str) -> Result<()> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(PASSWORD);
        std::fs::write(&file, pw)?;
        // Only SYSTEM and administrators may read it (students must not).
        let f = file.to_string_lossy().into_owned();
        let _ = sh::run(
            "icacls",
            &[
                &f,
                "/inheritance:r",
                "/grant:r",
                "*S-1-5-18:F",
                "*S-1-5-32-544:F",
            ],
        );
        Ok(())
    }

    fn persist(&self, name: &str, contents: &str) -> Result<()> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(name), contents)?;
        Ok(())
    }

    fn read_persistent(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(data_dir().join(name)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parsing() {
        assert_eq!(parse_list("Drives in Shadow Mode: C: D:"), vec!['C', 'D']);
        assert_eq!(parse_list("C\r\nE\r\n"), vec!['C', 'E']);
        assert!(parse_list("").is_empty());
        assert!(parse_list("No drive in Shadow Mode").is_empty());
    }

    #[test]
    fn exit_marker() {
        assert_eq!(split_exit("EXIT:0\nC: D:"), (0, "C: D:".to_string()));
        assert_eq!(split_exit("EXIT:5"), (5, String::new()));
        assert_eq!(split_exit("nothing").0, -1);
    }

    #[test]
    fn args() {
        assert_eq!(arg_with_value("/pwd", "123"), "/pwd:123");
        assert_eq!(arg_with_value("/pwd", ""), "/pwd:");
        assert_eq!(arg_with_value("/commit", "C:\\a b"), "/commit:\"C:\\a b\"");
    }

    #[test]
    fn reg_output() {
        let out = "HKEY_LOCAL_MACHINE\\SOFTWARE\\X\\Shadow Defender_is1\n    DisplayIcon    REG_SZ    C:\\Program Files\\Shadow Defender\\Defender.exe,0\n    InstallLocation    REG_SZ    C:\\Program Files\\Shadow Defender\\\n";
        let d = dirs_from_reg_output(out);
        assert!(
            d.iter()
                .all(|p| p.to_string_lossy().ends_with("Shadow Defender")),
            "{d:?}"
        );
        assert!(!d.is_empty());
    }
}
