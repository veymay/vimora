//! The student's clipboard, for the teacher who watches this computer live: whatever the
//! student copies (text, a picture, files) is read here once, and only when it changes.
//! The first look after the live view starts only remembers what is already there.

use arboard::Clipboard;
use image::ImageEncoder;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vimora_core::ClipData;

const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_FILES: u64 = 20 * 1024 * 1024;
const MAX_IMAGE: usize = 12 * 1024 * 1024;

pub struct Watch {
    board: Option<Clipboard>,
    last: Option<u64>,
}

impl Watch {
    pub fn new() -> Self {
        Self {
            board: None,
            last: None,
        }
    }

    /// The live view ended: the next one starts from scratch.
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// New content since the last look, if any (nothing on the first look).
    pub fn poll(&mut self) -> Option<ClipData> {
        if self.board.is_none() {
            self.board = Clipboard::new().ok();
        }
        let board = self.board.as_mut()?;
        let data = read(board);
        let mut h = DefaultHasher::new();
        match &data {
            Some(ClipData::Text(t)) => (0u8, t).hash(&mut h),
            Some(ClipData::Image(b)) => (1u8, b.len(), b.first(), b.last()).hash(&mut h),
            Some(ClipData::Files(f)) => {
                (2u8, f.iter().map(|(n, d)| (n, d.len())).collect::<Vec<_>>()).hash(&mut h)
            }
            None => 3u8.hash(&mut h),
        }
        let now = h.finish();
        let first = self.last.is_none();
        let same = self.last == Some(now);
        self.last = Some(now);
        if first || same {
            return None;
        }
        data
    }
}

fn read(board: &mut Clipboard) -> Option<ClipData> {
    if let Ok(paths) = board.get().file_list() {
        let mut total = 0u64;
        let mut files = Vec::new();
        for p in paths {
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            total += meta.len();
            if total > MAX_FILES {
                return None;
            }
            if let (Some(name), Ok(data)) = (p.file_name(), std::fs::read(&p)) {
                files.push((name.to_string_lossy().into_owned(), data));
            }
        }
        if !files.is_empty() {
            return Some(ClipData::Files(files));
        }
    }
    if let Ok(img) = board.get().image() {
        let mut png = Vec::new();
        let ok = image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &img.bytes,
                img.width as u32,
                img.height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .is_ok();
        if ok && png.len() <= MAX_IMAGE {
            return Some(ClipData::Image(png));
        }
    }
    match board.get().text() {
        Ok(t) if !t.is_empty() && t.len() <= MAX_TEXT => Some(ClipData::Text(t)),
        _ => None,
    }
}
