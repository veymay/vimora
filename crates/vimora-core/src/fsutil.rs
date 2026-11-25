//! Saving settings so that a crash, a power cut or a full disk can never cost them.
//!
//! A plain `fs::write` empties the file first: if the program dies in the middle, the file is
//! half-written, and a loader that "falls back to defaults" then throws everything away (and the
//! next save makes it permanent). Here the new content goes to a temporary file, is flushed to
//! disk and renamed over the old one (one step: the file is always either the old or the new
//! version); the previous version is kept as `<name>.bak`, and a file that cannot be read is
//! never silently replaced — the backup is tried, and the broken one is kept as `.corrupt`.

use serde::de::DeserializeOwned;
use std::io::Write;
use std::path::{Path, PathBuf};

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `data` to `path` atomically, keeping the previous content as `<path>.bak`.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = sibling(path, &format!(".{}.tmp", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    // Keep the last good version (a copy: the file itself stays in place until the rename).
    if path.exists() {
        let _ = std::fs::copy(path, sibling(path, ".bak"));
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Reads and parses a JSON file. A missing file is `None`; one that exists but cannot be parsed
/// is set aside as `<path>.corrupt` and the backup is used instead (`None` if that fails too).
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let main = std::fs::read_to_string(path).ok();
    if let Some(v) = main.as_deref().and_then(|s| serde_json::from_str(s).ok()) {
        return Some(v);
    }
    if main.is_some() {
        // Evidence for a person to look at; never overwritten silently.
        let _ = std::fs::rename(path, sibling(path, ".corrupt"));
    }
    let bak = std::fs::read_to_string(sibling(path, ".bak")).ok()?;
    serde_json::from_str(&bak).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "vimora-fsutil-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_replaces_whole_and_keeps_the_previous_as_backup() {
        let d = dir();
        let p = d.join("a.json");
        write_atomic(&p, b"{\"v\":1}").unwrap();
        write_atomic(&p, b"{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"v\":2}");
        assert_eq!(
            std::fs::read_to_string(sibling(&p, ".bak")).unwrap(),
            "{\"v\":1}"
        );
        // No temporary file is left behind.
        let left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(left.is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_broken_file_falls_back_to_the_backup_and_is_kept_for_inspection() {
        let d = dir();
        let p = d.join("b.json");
        write_atomic(&p, b"{\"n\":7}").unwrap();
        write_atomic(&p, b"{\"n\":8}").unwrap();
        // Simulate a crash in the middle of a plain write: half a file.
        std::fs::write(&p, b"{\"n\":").unwrap();
        let v: serde_json::Value = read_json(&p).expect("backup used");
        assert_eq!(v["n"], 7);
        assert!(sibling(&p, ".corrupt").exists(), "the broken file is kept");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn missing_everywhere_is_none() {
        let d = dir();
        assert!(read_json::<serde_json::Value>(&d.join("none.json")).is_none());
        let _ = std::fs::remove_dir_all(d);
    }
}
