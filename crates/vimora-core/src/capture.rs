//! Screen capture + JPEG encoding.
//!
//! * Windows: GDI BitBlt of the whole virtual desktop (all monitors).
//! * Linux/BSD: X11 `GetImage` on the root window (pure-Rust x11rb, no system -dev packages).
//!   On a pure Wayland session only XWayland windows are visible; use an X11 session on
//!   classroom PCs for full capture.

use anyhow::{anyhow, Result};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType, ExtendedColorType, RgbaImage};

/// Raw RGBA screen image.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Capturer {
    inner: imp::Capturer,
}

impl Capturer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: imp::Capturer::new()?,
        })
    }

    pub fn capture(&mut self) -> Result<Frame> {
        self.inner.capture()
    }

    /// Screen size in pixels (used to map normalised remote-control coordinates).
    pub fn screen_size(&self) -> (u32, u32) {
        self.inner.screen_size()
    }
}

/// Downscale to `max_width` (keeping aspect ratio) and JPEG-encode.
pub fn encode_jpeg(frame: &Frame, max_width: u32, quality: u8) -> Result<(u32, u32, Vec<u8>)> {
    let img = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or_else(|| anyhow!("bad frame buffer"))?;
    let img = if max_width > 0 && frame.width > max_width {
        let h = (frame.height as u64 * max_width as u64 / frame.width as u64).max(1) as u32;
        image::imageops::resize(&img, max_width, h, FilterType::Triangle)
    } else {
        img
    };
    let (w, h) = img.dimensions();
    let rgb: Vec<u8> = img
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut out = Vec::with_capacity((w * h / 4) as usize);
    JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)).encode(
        &rgb,
        w,
        h,
        ExtendedColorType::Rgb8,
    )?;
    Ok((w, h, out))
}

/// Decode a JPEG into RGBA (used by the console for thumbnails and by the student for "show").
pub fn decode_jpeg(data: &[u8]) -> Result<Frame> {
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)?.to_rgba8();
    let (width, height) = img.dimensions();
    Ok(Frame {
        width,
        height,
        rgba: img.into_raw(),
    })
}

// ---------------------------------------------------------------------------------------------
#[cfg(windows)]
mod imp {
    use super::Frame;
    use anyhow::{bail, Result};
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub struct Capturer;

    impl Capturer {
        pub fn new() -> Result<Self> {
            Ok(Self)
        }

        fn rect() -> (i32, i32, i32, i32) {
            unsafe {
                (
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN),
                )
            }
        }

        pub fn screen_size(&self) -> (u32, u32) {
            let (_, _, w, h) = Self::rect();
            (w.max(1) as u32, h.max(1) as u32)
        }

        pub fn capture(&mut self) -> Result<Frame> {
            let (x, y, w, h) = Self::rect();
            if w <= 0 || h <= 0 {
                bail!("no screen");
            }
            unsafe {
                let screen = GetDC(null_mut());
                if screen.is_null() {
                    bail!("GetDC failed (no interactive desktop?)");
                }
                let mem = CreateCompatibleDC(screen);
                let bmp = CreateCompatibleBitmap(screen, w, h);
                let old = SelectObject(mem, bmp as _);
                let ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);

                let mut bi: BITMAPINFO = zeroed();
                bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
                bi.bmiHeader.biWidth = w;
                bi.bmiHeader.biHeight = -h; // top-down
                bi.bmiHeader.biPlanes = 1;
                bi.bmiHeader.biBitCount = 32;
                bi.bmiHeader.biCompression = BI_RGB as _;
                let mut buf = vec![0u8; (w * h * 4) as usize];
                let lines = GetDIBits(
                    mem,
                    bmp,
                    0,
                    h as u32,
                    buf.as_mut_ptr() as _,
                    &mut bi,
                    DIB_RGB_COLORS,
                );

                SelectObject(mem, old);
                DeleteObject(bmp as _);
                DeleteDC(mem);
                ReleaseDC(null_mut(), screen);

                if ok == 0 || lines == 0 {
                    bail!("BitBlt/GetDIBits failed");
                }
                for px in buf.as_chunks_mut::<4>().0 {
                    px.swap(0, 2); // BGRA -> RGBA
                    px[3] = 255;
                }
                Ok(Frame {
                    width: w as u32,
                    height: h as u32,
                    rgba: buf,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::Frame;
    use anyhow::{bail, Context, Result};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, Window};
    use x11rb::rust_connection::RustConnection;

    pub struct Capturer {
        conn: RustConnection,
        root: Window,
        width: u16,
        height: u16,
    }

    impl Capturer {
        pub fn new() -> Result<Self> {
            let (conn, screen_num) =
                x11rb::connect(None).context("cannot connect to X11 display")?;
            let screen = &conn.setup().roots[screen_num];
            let (root, width, height) =
                (screen.root, screen.width_in_pixels, screen.height_in_pixels);
            Ok(Self {
                conn,
                root,
                width,
                height,
            })
        }

        pub fn screen_size(&self) -> (u32, u32) {
            (self.width as u32, self.height as u32)
        }

        pub fn capture(&mut self) -> Result<Frame> {
            // Re-read geometry in case of resolution change.
            if let Ok(g) = self.conn.get_geometry(self.root)?.reply() {
                self.width = g.width;
                self.height = g.height;
            }
            let reply = match self
                .conn
                .get_image(
                    ImageFormat::Z_PIXMAP,
                    self.root,
                    0,
                    0,
                    self.width,
                    self.height,
                    !0,
                )?
                .reply()
            {
                Ok(r) => r,
                Err(e) if std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland") => {
                    bail!("Wayland session: screen capture needs an X11 (Xorg) session ({e})")
                }
                Err(e) => return Err(e.into()),
            };
            let (w, h) = (self.width as usize, self.height as usize);
            let data = reply.data;
            if data.len() < w * h * 4 {
                bail!("unsupported X11 visual depth {}", reply.depth);
            }
            let mut rgba = Vec::with_capacity(w * h * 4);
            for px in data.as_chunks::<4>().0.iter().take(w * h) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
            Ok(Frame {
                width: w as u32,
                height: h as u32,
                rgba,
            })
        }
    }
}

// ---------------------------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod imp {
    use super::Frame;
    use anyhow::{bail, Result};

    pub struct Capturer;
    impl Capturer {
        pub fn new() -> Result<Self> {
            bail!("screen capture is not implemented on macOS yet")
        }
        pub fn screen_size(&self) -> (u32, u32) {
            (1, 1)
        }
        pub fn capture(&mut self) -> Result<Frame> {
            bail!("unsupported")
        }
    }
}
