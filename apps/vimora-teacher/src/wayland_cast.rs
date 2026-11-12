//! Screen capture on Wayland, for presenting the teacher's screen.
//!
//! X11 lets any program read the screen; Wayland does not. The supported way is the desktop
//! portal: it asks the teacher once ("share your screen"), then hands out a PipeWire stream.
//! Reading PipeWire directly would need its C library at build time, so the frames are read
//! through GStreamer's command-line tool, which every Ubuntu desktop can run:
//!
//! `gst-launch-1.0 pipewiresrc ! … ! jpegenc ! fdsink` → JPEG frames on its stdout.
//!
//! The permission is remembered (restore token), so the question is asked only the first time.

use anyhow::{anyhow, bail, Context, Result};
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

/// One JPEG frame of the screen.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

/// A running screen cast. Dropping it stops the stream and ends the portal session.
pub struct Cast {
    frames: Receiver<Frame>,
    stop: Arc<AtomicBool>,
}

impl Drop for Cast {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

fn token_file() -> Option<std::path::PathBuf> {
    crate::config::dir().map(|d| d.join("screencast-token"))
}

impl Cast {
    /// Asks the portal for the screen (a system dialog the first time) and starts the stream.
    /// Blocks until the teacher answered, at most two minutes.
    pub fn start() -> Result<Cast> {
        if Command::new("gst-launch-1.0")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            bail!("NO_GSTREAMER");
        }
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let (frame_tx, frames) = mpsc::sync_channel::<Frame>(2);
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("wayland-cast".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.into()));
                        return;
                    }
                };
                rt.block_on(async move {
                    match open_stream().await {
                        Ok((proxy, session, node, fd, size)) => {
                            let child = spawn_gst(node, &fd, size);
                            drop(fd); // the child has its own copy
                            match child {
                                Ok(mut child) => {
                                    let _ = ready_tx.send(Ok(()));
                                    let out = child.stdout.take();
                                    let stop3 = stop2.clone();
                                    let reader = std::thread::spawn(move || {
                                        if let Some(out) = out {
                                            read_frames(out, &frame_tx, &stop3);
                                        }
                                        stop3.store(true, Ordering::Relaxed);
                                    });
                                    // Keep the portal session (and so the stream) alive.
                                    while !stop2.load(Ordering::Relaxed) {
                                        tokio::time::sleep(Duration::from_millis(150)).await;
                                    }
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    let _ = reader.join();
                                    let _ = session.close().await;
                                    drop(proxy);
                                }
                                Err(e) => {
                                    let _ = ready_tx.send(Err(e));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                        }
                    }
                });
            })?;
        match ready_rx.recv_timeout(Duration::from_secs(120)) {
            Ok(Ok(())) => Ok(Cast { frames, stop }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err(anyhow!("CAST_TIMEOUT"))
            }
        }
    }

    /// The newest frame since the last call. `Err` once the stream has ended.
    pub fn latest(&self) -> Result<Option<Frame>> {
        let mut newest = None;
        loop {
            match self.frames.try_recv() {
                Ok(f) => newest = Some(f),
                Err(mpsc::TryRecvError::Empty) => return Ok(newest),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return match newest {
                        Some(f) => Ok(Some(f)),
                        None => Err(anyhow!("screen cast ended")),
                    }
                }
            }
        }
    }
}

type Opened<'a> = (
    Screencast<'a>,
    ashpd::desktop::Session<'a, Screencast<'a>>,
    u32,
    OwnedFd,
    Option<(i32, i32)>,
);

async fn open_stream<'a>() -> Result<Opened<'a>> {
    let proxy = Screencast::new().await.context("PORTAL")?;
    let session = proxy.create_session().await.context("PORTAL")?;
    let saved = token_file().and_then(|p| std::fs::read_to_string(p).ok());
    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor.into(),
            false,
            saved.as_deref().map(str::trim).filter(|t| !t.is_empty()),
            PersistMode::ExplicitlyRevoked,
        )
        .await
        .context("PORTAL")?;
    let streams = proxy
        .start(&session, None)
        .await
        .context("PORTAL")?
        .response()
        .map_err(|_| anyhow!("CAST_DENIED"))?;
    if let (Some(tok), Some(path)) = (streams.restore_token(), token_file()) {
        let _ = std::fs::write(path, tok);
    }
    let stream = streams
        .streams()
        .first()
        .ok_or_else(|| anyhow!("CAST_DENIED"))?;
    let (node, size) = (stream.pipe_wire_node_id(), stream.size());
    let fd = proxy
        .open_pipe_wire_remote(&session)
        .await
        .context("PORTAL")?;
    Ok((proxy, session, node, fd, size))
}

/// Starts GStreamer reading the PipeWire node (the portal's connection is passed as fd 3) and
/// writing JPEG frames to its stdout.
fn spawn_gst(node: u32, fd: &OwnedFd, size: Option<(i32, i32)>) -> Result<Child> {
    use std::os::unix::process::CommandExt;
    // Full HD is plenty for a classroom; keep the aspect ratio and even dimensions.
    let scale = match size {
        Some((w, h)) if w > 1920 && h > 0 => {
            let nh = ((h as f32 * 1920.0 / w as f32) as i32) & !1;
            format!("! videoscale ! video/x-raw,width=1920,height={nh} ")
        }
        _ => String::new(),
    };
    let pipeline = format!(
        "pipewiresrc fd=3 path={node} do-timestamp=true ! videorate ! video/x-raw,framerate=8/1 \
         ! videoconvert {scale}! jpegenc quality=72 ! fdsink fd=1"
    );
    let raw = fd.as_raw_fd();
    let mut cmd = Command::new("gst-launch-1.0");
    cmd.arg("-q")
        .args(pipeline.split_whitespace())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: only async-signal-safe calls (prctl / dup2 / fcntl) between fork and exec.
    unsafe {
        cmd.pre_exec(move || {
            // Never outlive the teacher (a crash must not leave the screen being recorded).
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            if raw == 3 {
                let flags = libc::fcntl(3, libc::F_GETFD);
                libc::fcntl(3, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            } else if libc::dup2(raw, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn().map_err(|_| anyhow!("NO_GSTREAMER"))
}

/// Splits the byte stream into JPEG images and sends them on (dropping frames when the
/// receiver is busy: only the newest matters).
fn read_frames(mut out: impl Read, tx: &mpsc::SyncSender<Frame>, stop: &AtomicBool) {
    let mut buf: Vec<u8> = Vec::with_capacity(1 << 20);
    let mut chunk = vec![0u8; 256 * 1024];
    while !stop.load(Ordering::Relaxed) {
        let n = match out.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some((end, width, height)) = jpeg_end(&buf) {
            let jpeg: Vec<u8> = buf.drain(..end).collect();
            match tx.try_send(Frame {
                width,
                height,
                jpeg,
            }) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => return,
            }
        }
        if buf.len() > 64 << 20 {
            buf.clear(); // lost sync: start over at the next image
        }
    }
}

/// If `buf` starts with one complete JPEG, returns (its length, width, height).
/// Garbage before the first image is not tolerated: the stream starts with one.
pub fn jpeg_end(buf: &[u8]) -> Option<(usize, u32, u32)> {
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return None;
    }
    let (mut i, mut w, mut h) = (2usize, 0u32, 0u32);
    // Header segments up to the start of the compressed data.
    loop {
        if i + 4 > buf.len() {
            return None;
        }
        if buf[i] != 0xFF {
            return None;
        }
        let marker = buf[i + 1];
        let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        if (0xC0..=0xC2).contains(&marker) && i + 9 <= buf.len() {
            h = u16::from_be_bytes([buf[i + 5], buf[i + 6]]) as u32;
            w = u16::from_be_bytes([buf[i + 7], buf[i + 8]]) as u32;
        }
        i += 2 + len;
        if marker == 0xDA {
            break;
        }
    }
    // Compressed data: 0xFF is always followed by 0x00 (stuffing) or a restart marker, until
    // the end-of-image marker.
    while i + 1 < buf.len() {
        if buf[i] == 0xFF {
            match buf[i + 1] {
                0xD9 => return Some((i + 2, w, h)),
                _ => i += 2,
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::jpeg_end;

    /// Real encoder output: `cargo test -p vimora-teacher gst_stream -- --ignored`.
    #[test]
    #[ignore]
    fn gst_stream_splits_into_frames() {
        let out = std::process::Command::new("gst-launch-1.0")
            .args("-q videotestsrc num-buffers=5 ! video/x-raw,width=320,height=240 ! jpegenc ! fdsink fd=1".split(' '))
            .output()
            .expect("gst-launch-1.0");
        let mut buf = out.stdout;
        let mut n = 0;
        while let Some((end, w, h)) = jpeg_end(&buf) {
            assert_eq!((w, h), (320, 240));
            buf.drain(..end);
            n += 1;
        }
        assert_eq!(n, 5);
        assert!(buf.is_empty());
    }

    /// Asks the portal for the screen (a dialog appears) and reads a few frames:
    /// `cargo test -p vimora-teacher live_cast -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_cast() {
        let cast = super::Cast::start().expect("cast");
        let t = std::time::Instant::now();
        let mut n = 0;
        while t.elapsed() < std::time::Duration::from_secs(6) {
            if let Some(f) = cast.latest().expect("stream") {
                n += 1;
                println!("frame {n}: {}x{} {} bytes", f.width, f.height, f.jpeg.len());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(n > 0, "no frames");
    }

    #[test]
    fn finds_the_end_of_an_image() {
        // SOI, SOF0 (2x1), SOS, data with a stuffed FF and a restart marker, EOI, next SOI.
        let mut j = vec![0xFF, 0xD8];
        j.extend([0xFF, 0xC0, 0, 8, 8, 0, 1, 0, 2, 1]);
        j.extend([0xFF, 0xDA, 0, 2]);
        j.extend([1, 2, 0xFF, 0x00, 3, 0xFF, 0xD0, 4]);
        j.extend([0xFF, 0xD9]);
        let len = j.len();
        let mut two = j.clone();
        two.extend([0xFF, 0xD8, 0xFF]);
        assert_eq!(jpeg_end(&two), Some((len, 2, 1)));
        assert_eq!(jpeg_end(&j[..len - 1]), None);
        assert_eq!(jpeg_end(&[0, 1, 2, 3]), None);
    }
}
