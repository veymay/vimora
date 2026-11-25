//! The teacher's log: to the terminal and to `teacher.log` next to the settings, so that
//! something that went wrong in a classroom can be looked at afterwards (and sent along). The
//! file is kept small: at 2 MB it becomes `teacher.log.1` and a new one starts.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const MAX: u64 = 2 * 1024 * 1024;

struct Inner {
    file: File,
    path: PathBuf,
    size: u64,
}

static SINK: OnceLock<Mutex<Option<Inner>>> = OnceLock::new();

fn open(path: &PathBuf) -> Option<Inner> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some(Inner {
        file,
        path: path.clone(),
        size,
    })
}

/// Where the log file is (also for "open the folder").
pub fn path() -> Option<PathBuf> {
    crate::config::dir().map(|d| d.join("teacher.log"))
}

/// Opens the log file; call once at start.
pub fn init() {
    let inner = path().and_then(|p| {
        let _ = std::fs::create_dir_all(p.parent()?);
        open(&p)
    });
    let _ = SINK.set(Mutex::new(inner));
}

/// A `MakeWriter` for tracing: terminal + file.
#[derive(Clone, Copy)]
pub struct Tee;

pub struct TeeWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Tee {
    type Writer = TeeWriter;
    fn make_writer(&'a self) -> TeeWriter {
        TeeWriter
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(m) = SINK.get() {
            if let Ok(mut g) = m.lock() {
                if let Some(inner) = g.as_mut() {
                    if inner.size > MAX {
                        let old = inner.path.with_extension("log.1");
                        let _ = std::fs::rename(&inner.path, old);
                        if let Some(n) = open(&inner.path) {
                            *inner = n;
                        }
                    }
                    if inner.file.write_all(buf).is_ok() {
                        inner.size += buf.len() as u64;
                    }
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
