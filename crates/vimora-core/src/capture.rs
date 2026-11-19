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
    /// Draw the mouse pointer into the frames (casting and live view: the screen shows where
    /// the pointer is).
    cursor: bool,
}

impl Capturer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: imp::Capturer::new()?,
            cursor: false,
        })
    }

    /// Whether the mouse pointer is drawn into the frames.
    pub fn set_cursor(&mut self, on: bool) {
        self.cursor = on;
    }

    pub fn capture(&mut self) -> Result<Frame> {
        self.inner.capture(self.cursor)
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

/// Decode a JPEG into RGBA (used by the teacher for thumbnails and by the student for "show").
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

    /// The mouse pointer is not part of a BitBlt of the screen: draw it on top (where its hot
    /// spot is), with whatever shape it has now.
    unsafe fn draw_pointer(mem: HDC, origin_x: i32, origin_y: i32) {
        let mut ci: CURSORINFO = zeroed();
        ci.cbSize = size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut ci) == 0 || ci.flags & CURSOR_SHOWING == 0 || ci.hCursor.is_null() {
            return;
        }
        let mut ii: ICONINFO = zeroed();
        let (hx, hy) = if GetIconInfo(ci.hCursor, &mut ii) != 0 {
            let hot = (ii.xHotspot as i32, ii.yHotspot as i32);
            if !ii.hbmMask.is_null() {
                DeleteObject(ii.hbmMask as _);
            }
            if !ii.hbmColor.is_null() {
                DeleteObject(ii.hbmColor as _);
            }
            hot
        } else {
            (0, 0)
        };
        DrawIconEx(
            mem,
            ci.ptScreenPos.x - origin_x - hx,
            ci.ptScreenPos.y - origin_y - hy,
            ci.hCursor,
            0,
            0,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }

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

        pub fn capture(&mut self, cursor: bool) -> Result<Frame> {
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
                if cursor {
                    draw_pointer(mem, x, y);
                }

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
    use x11rb::protocol::xfixes::ConnectionExt as _;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, Window};
    use x11rb::rust_connection::RustConnection;

    pub struct Capturer {
        conn: RustConnection,
        root: Window,
        width: u16,
        height: u16,
        /// The XFixes extension (pointer image) is there.
        xfixes: bool,
    }

    impl Capturer {
        pub fn new() -> Result<Self> {
            let (conn, screen_num) =
                x11rb::connect(None).context("cannot connect to X11 display")?;
            let xfixes = conn
                .xfixes_query_version(5, 0)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some();
            let screen = &conn.setup().roots[screen_num];
            let (root, width, height) =
                (screen.root, screen.width_in_pixels, screen.height_in_pixels);
            Ok(Self {
                conn,
                root,
                width,
                height,
                xfixes,
            })
        }

        pub fn screen_size(&self) -> (u32, u32) {
            (self.width as u32, self.height as u32)
        }

        /// Blends the pointer (premultiplied ARGB from XFixes) into the frame.
        fn draw_pointer(&self, rgba: &mut [u8], w: usize, h: usize) {
            let Some(ci) = self
                .conn
                .xfixes_get_cursor_image()
                .ok()
                .and_then(|c| c.reply().ok())
            else {
                return;
            };
            let (x0, y0) = (ci.x as i32 - ci.xhot as i32, ci.y as i32 - ci.yhot as i32);
            for cy in 0..ci.height as i32 {
                for cx in 0..ci.width as i32 {
                    let (px, py) = (x0 + cx, y0 + cy);
                    if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                        continue;
                    }
                    let argb = ci.cursor_image[(cy * ci.width as i32 + cx) as usize];
                    let a = (argb >> 24) & 0xff;
                    if a == 0 {
                        continue;
                    }
                    let d = (py as usize * w + px as usize) * 4;
                    let src = [(argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff];
                    for k in 0..3 {
                        // Premultiplied: dst = src + dst * (1 - a)
                        let v = src[k] + rgba[d + k] as u32 * (255 - a) / 255;
                        rgba[d + k] = v.min(255) as u8;
                    }
                }
            }
        }

        pub fn capture(&mut self, cursor: bool) -> Result<Frame> {
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
                    // The teacher turns this marker into a plain-language hint.
                    let _ = &e;
                    bail!("WAYLAND")
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
            if cursor && self.xfixes {
                self.draw_pointer(&mut rgba, w, h);
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
        pub fn capture(&mut self, _cursor: bool) -> Result<Frame> {
            bail!("unsupported")
        }
    }
}
