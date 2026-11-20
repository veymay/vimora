//! Video encoders for the stream. [`VideoEncoder`] is what the session needs; the production one
//! runs GStreamer's `vp8enc` as a child process (raw frames in on stdin, one file per encoded
//! frame out), so no C library is linked into Vimora.

use crate::stream::EncodedFrame;
use anyhow::{anyhow, bail, Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

/// Turns raw RGBA frames into encoded ones.
pub trait VideoEncoder: Send {
    /// One raw frame (`width * height * 4` bytes).
    fn push(&mut self, rgba: &[u8]) -> Result<()>;
    /// Encoded frames that are ready, oldest first.
    fn poll(&mut self) -> Vec<EncodedFrame>;
}

/// Makes an encoder for `(width, height, fps, kbps)`.
pub type EncoderFactory =
    Arc<dyn Fn(u32, u32, u32, u32) -> Result<Box<dyn VideoEncoder>> + Send + Sync>;

/// The production factory: VP8 through GStreamer.
pub fn gstreamer_vp8() -> EncoderFactory {
    Arc::new(|w, h, fps, kbps| Ok(Box::new(GstVp8::start(w, h, fps, kbps)?)))
}

struct GstVp8 {
    child: Child,
    stdin: ChildStdin,
    dir: PathBuf,
    next: u64,
    frame_bytes: usize,
}

impl GstVp8 {
    fn start(w: u32, h: u32, fps: u32, kbps: u32) -> Result<Self> {
        let frame_bytes = (w as usize) * (h as usize) * 4;
        let dir = std::env::temp_dir().join(format!(
            "vimora-cast-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir)?;
        let launch = format!(
            "-q fdsrc fd=0 blocksize={frame_bytes} ! \
             rawvideoparse width={w} height={h} format=rgba framerate={fps}/1 ! videoconvert ! \
             vp8enc deadline=1 cpu-used=8 end-usage=cbr target-bitrate={bps} \
             keyframe-max-dist={fps} lag-in-frames=0 threads=4 ! \
             multifilesink location={loc} sync=false",
            bps = kbps as u64 * 1000,
            loc = dir.join("f%08d.vp8").display()
        );
        let mut child = Command::new("gst-launch-1.0")
            .args(launch.split_whitespace())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("GStreamer (gst-launch-1.0) is not installed")?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("no encoder input"))?;
        Ok(Self {
            child,
            stdin,
            dir,
            next: 0,
            frame_bytes,
        })
    }
}

impl VideoEncoder for GstVp8 {
    fn push(&mut self, rgba: &[u8]) -> Result<()> {
        if rgba.len() != self.frame_bytes {
            bail!(
                "frame of {} bytes, expected {}",
                rgba.len(),
                self.frame_bytes
            );
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            bail!("the encoder stopped ({status})");
        }
        self.stdin
            .write_all(rgba)
            .context("the encoder closed its input")?;
        Ok(())
    }

    fn poll(&mut self) -> Vec<EncodedFrame> {
        let mut out = vec![];
        loop {
            let path = self.dir.join(format!("f{:08}.vp8", self.next));
            let Ok(len1) = std::fs::metadata(&path).map(|m| m.len()) else {
                break;
            };
            // The file may still be being written: take it once its size stands still.
            std::thread::sleep(Duration::from_millis(1));
            let len2 = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if len1 == 0 || len1 != len2 {
                break;
            }
            let Ok(data) = std::fs::read(&path) else {
                break;
            };
            let _ = std::fs::remove_file(&path);
            self.next += 1;
            // A VP8 frame tag's lowest bit is 0 for a key frame.
            let key = data.first().is_some_and(|b| b & 1 == 0);
            out.push(EncodedFrame { data, key });
        }
        out
    }
}

impl Drop for GstVp8 {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
