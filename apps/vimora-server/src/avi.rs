//! Screen recording: the JPEG frames a student PC sends while it is watched, written as an
//! MJPEG AVI that every video player opens. No encoder needed: the frames already are JPEG.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct AviWriter {
    file: File,
    width: u32,
    height: u32,
    /// (offset from the `movi` fourcc, size) of every frame, for the index.
    index: Vec<(u32, u32)>,
    movi_start: u64,
    max_frame: u32,
    fps: u32,
}

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

impl AviWriter {
    pub fn create(path: &Path, width: u32, height: u32, fps: u32) -> std::io::Result<Self> {
        let mut file = File::create(path)?;
        // Header with placeholders, patched in `finish`.
        file.write_all(&Self::header(width, height, fps, 0, 0, 0))?;
        let movi_start = file.stream_position()? - 4; // position of "movi"
        Ok(Self {
            file,
            width,
            height,
            index: vec![],
            movi_start,
            max_frame: 0,
            fps,
        })
    }

    fn header(w: u32, h: u32, fps: u32, frames: u32, max_frame: u32, movi_size: u32) -> Vec<u8> {
        let mut avih = vec![];
        avih.extend(u32le(1_000_000 / fps.max(1))); // microseconds per frame
        avih.extend(u32le(max_frame * fps)); // max bytes per second
        avih.extend(u32le(0)); // padding granularity
        avih.extend(u32le(0x10)); // AVIF_HASINDEX
        avih.extend(u32le(frames));
        avih.extend(u32le(0)); // initial frames
        avih.extend(u32le(1)); // streams
        avih.extend(u32le(max_frame)); // suggested buffer size
        avih.extend(u32le(w));
        avih.extend(u32le(h));
        avih.extend([0u8; 16]); // reserved

        let mut strh = vec![];
        strh.extend(b"vids");
        strh.extend(b"MJPG");
        strh.extend(u32le(0)); // flags
        strh.extend([0u8; 4]); // priority + language
        strh.extend(u32le(0)); // initial frames
        strh.extend(u32le(1)); // scale
        strh.extend(u32le(fps)); // rate
        strh.extend(u32le(0)); // start
        strh.extend(u32le(frames)); // length
        strh.extend(u32le(max_frame)); // suggested buffer size
        strh.extend(u32le(u32::MAX)); // quality
        strh.extend(u32le(0)); // sample size
        strh.extend([0u8; 4]); // rcFrame left, top
        strh.extend((w as u16).to_le_bytes());
        strh.extend((h as u16).to_le_bytes());

        let mut strf = vec![];
        strf.extend(u32le(40));
        strf.extend(u32le(w));
        strf.extend(u32le(h));
        strf.extend(1u16.to_le_bytes()); // planes
        strf.extend(24u16.to_le_bytes()); // bits
        strf.extend(b"MJPG");
        strf.extend(u32le(w * h * 3));
        strf.extend([0u8; 16]); // ppm x/y, colours used/important

        let chunk = |id: &[u8], data: &[u8]| {
            let mut c = id.to_vec();
            c.extend(u32le(data.len() as u32));
            c.extend(data);
            c
        };
        let list = |kind: &[u8], body: &[u8]| {
            let mut c = b"LIST".to_vec();
            c.extend(u32le(body.len() as u32 + 4));
            c.extend(kind);
            c.extend(body);
            c
        };
        let mut strl = chunk(b"strh", &strh);
        strl.extend(chunk(b"strf", &strf));
        let mut hdrl = chunk(b"avih", &avih);
        hdrl.extend(list(b"strl", &strl));
        let hdrl = list(b"hdrl", &hdrl);

        let mut out = b"RIFF".to_vec();
        out.extend(u32le(0)); // patched
        out.extend(b"AVI ");
        out.extend(hdrl);
        out.extend(b"LIST");
        out.extend(u32le(movi_size + 4));
        out.extend(b"movi");
        out
    }

    pub fn frame(&mut self, jpeg: &[u8]) -> std::io::Result<()> {
        let pos = self.file.stream_position()?;
        self.file.write_all(b"00dc")?;
        self.file.write_all(&u32le(jpeg.len() as u32))?;
        self.file.write_all(jpeg)?;
        if jpeg.len() % 2 == 1 {
            self.file.write_all(&[0])?;
        }
        self.index
            .push(((pos - self.movi_start) as u32, jpeg.len() as u32));
        self.max_frame = self.max_frame.max(jpeg.len() as u32);
        Ok(())
    }

    pub fn frames(&self) -> usize {
        self.index.len()
    }

    /// Writes the index and the real sizes.
    pub fn finish(mut self) -> std::io::Result<()> {
        let movi_end = self.file.stream_position()?;
        let mut idx = vec![];
        for (off, size) in &self.index {
            idx.extend(b"00dc");
            idx.extend(u32le(0x10)); // keyframe
            idx.extend(u32le(*off));
            idx.extend(u32le(*size));
        }
        self.file.write_all(b"idx1")?;
        self.file.write_all(&u32le(idx.len() as u32))?;
        self.file.write_all(&idx)?;
        let end = self.file.stream_position()?;
        let movi_size = (movi_end - self.movi_start - 4) as u32;
        let header = Self::header(
            self.width,
            self.height,
            self.fps,
            self.index.len() as u32,
            self.max_frame,
            movi_size,
        );
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.seek(SeekFrom::Start(4))?;
        self.file.write_all(&u32le((end - 8) as u32))?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_a_valid_riff() {
        let dir = std::env::temp_dir().join("vimora-avi-test.avi");
        let mut w = super::AviWriter::create(&dir, 4, 2, 8).unwrap();
        w.frame(&[0xff, 0xd8, 1, 0xff, 0xd9]).unwrap();
        w.frame(&[0xff, 0xd8, 2, 2, 0xff, 0xd9]).unwrap();
        w.finish().unwrap();
        let b = std::fs::read(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize,
            b.len() - 8
        );
        assert!(b.windows(4).any(|w| w == b"idx1"));
    }
}
