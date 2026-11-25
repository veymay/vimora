//! Screen capture + JPEG encoding.
//!
//! * Windows: GDI BitBlt of the whole virtual desktop (all monitors).
//! * Linux/BSD: X11 `GetImage` on the root window (pure-Rust x11rb, no system -dev packages).
//!   On a pure Wayland session only XWayland windows are visible; use an X11 session on
//!   classroom PCs for full capture.

use anyhow::{anyhow, Result};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType};

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

/// Shrinks an RGBA picture to `tw` pixels wide by averaging the source pixels that fall into each
/// output pixel (a box filter): every source pixel is read about once, which is several times
/// faster than the general resampling filters and looks right for thumbnails and screens.
/// Returns the new height and RGB pixels (3 bytes each).
fn downscale_box(src: &[u8], w: usize, h: usize, tw: usize) -> (usize, Vec<u8>) {
    let th = (h * tw / w).max(1);
    let mut out = vec![0u8; tw * th * 3];
    // Source column range of each output column.
    let xr: Vec<(usize, usize)> = (0..tw)
        .map(|x| {
            let x0 = x * w / tw;
            let x1 = (((x + 1) * w).div_ceil(tw)).clamp(x0 + 1, w);
            (x0, x1)
        })
        .collect();
    let mut acc = vec![0u32; tw * 4];
    for y in 0..th {
        let y0 = y * h / th;
        let y1 = (((y + 1) * h).div_ceil(th)).clamp(y0 + 1, h);
        acc.iter_mut().for_each(|a| *a = 0);
        for sy in y0..y1 {
            let row = &src[sy * w * 4..(sy + 1) * w * 4];
            for (x, &(x0, x1)) in xr.iter().enumerate() {
                let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
                for px in row[x0 * 4..x1 * 4].as_chunks::<4>().0 {
                    r += px[0] as u32;
                    g += px[1] as u32;
                    b += px[2] as u32;
                }
                acc[x * 4] += r;
                acc[x * 4 + 1] += g;
                acc[x * 4 + 2] += b;
                acc[x * 4 + 3] += (x1 - x0) as u32;
            }
        }
        let orow = &mut out[y * tw * 3..(y + 1) * tw * 3];
        for (x, o) in orow.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            let n = acc[x * 4 + 3].max(1);
            o[0] = (acc[x * 4] / n) as u8;
            o[1] = (acc[x * 4 + 1] / n) as u8;
            o[2] = (acc[x * 4 + 2] / n) as u8;
        }
    }
    (th, out)
}

/// Downscale to `max_width` (keeping aspect ratio) and JPEG-encode.
pub fn encode_jpeg(frame: &Frame, max_width: u32, quality: u8) -> Result<(u32, u32, Vec<u8>)> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w == 0 || h == 0 || frame.rgba.len() < w * h * 4 {
        return Err(anyhow!("bad frame buffer"));
    }
    let (ow, oh, rgb): (usize, usize, Vec<u8>) = if max_width > 0 && frame.width > max_width {
        let (th, px) = downscale_box(&frame.rgba, w, h, max_width as usize);
        (max_width as usize, th, px)
    } else {
        // No shrinking: only the alpha byte is dropped.
        let mut rgb = vec![0u8; w * h * 3];
        for (o, p) in rgb
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(frame.rgba.as_chunks::<4>().0)
        {
            o.copy_from_slice(&p[..3]);
        }
        (w, h, rgb)
    };
    let mut out = Vec::with_capacity(ow * oh / 4);
    JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100)).encode(
        &rgb,
        ow as u32,
        oh as u32,
        ExtendedColorType::Rgb8,
    )?;
    Ok((ow as u32, oh as u32, out))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Frame {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let c = f(x, y);
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        Frame {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn a_solid_picture_stays_solid_and_keeps_its_proportions() {
        let f = frame(1920, 1080, |_, _| [200, 40, 90]);
        let (w, h, jpeg) = encode_jpeg(&f, 480, 60).unwrap();
        assert_eq!((w, h), (480, 270));
        let d = decode_jpeg(&jpeg).unwrap();
        assert_eq!((d.width, d.height), (480, 270));
        let mid = ((135 * 480 + 240) * 4) as usize;
        for (got, want) in d.rgba[mid..mid + 3].iter().zip([200u8, 40, 90]) {
            assert!((*got as i32 - want as i32).abs() <= 6, "{got} vs {want}");
        }
    }

    #[test]
    fn fine_detail_is_averaged_not_dropped() {
        // One pixel black, one white, alternating: the average is grey.
        let f = frame(800, 100, |x, _| if x % 2 == 0 { [0; 3] } else { [255; 3] });
        let (th, px) = downscale_box(&f.rgba, 800, 100, 200);
        assert_eq!(th, 25);
        let v = px[(10 * 200 + 50) * 3] as i32;
        assert!((v - 127).abs() <= 2, "{v}");
    }

    #[test]
    fn odd_ratios_and_tiny_targets_work() {
        let f = frame(1366, 768, |x, y| [(x % 256) as u8, (y % 256) as u8, 7]);
        for tw in [1366usize, 1365, 1600, 1000, 481, 7, 1] {
            let (th, px) = downscale_box(&f.rgba, 1366, 768, tw.min(1366));
            assert_eq!(px.len(), tw.min(1366) * th * 3);
        }
        // Smaller than the limit: unchanged size.
        let (w, h, _) = encode_jpeg(&f, 1600, 70).unwrap();
        assert_eq!((w, h), (1366, 768));
        assert!(encode_jpeg(
            &Frame {
                width: 4,
                height: 4,
                rgba: vec![0; 8]
            },
            0,
            50
        )
        .is_err());
    }

    /// `cargo test -p vimora-core --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed_of_a_1080p_thumbnail_and_live_frame() {
        let f = frame(1920, 1080, |x, y| {
            [(x ^ y) as u8, (x * 3) as u8, (y * 5) as u8]
        });
        for (name, mw, q) in [("thumbnail 480", 480, 55), ("live 1600", 1600, 70)] {
            let t = std::time::Instant::now();
            let n = 20;
            let mut len = 0;
            for _ in 0..n {
                len = encode_jpeg(&f, mw, q).unwrap().2.len();
            }
            println!(
                "{name}: {:.1} ms per frame, {} KB",
                t.elapsed().as_secs_f64() * 1000.0 / n as f64,
                len / 1024
            );
            // The way it was before (copy, general resize filter, iterator conversion).
            let t = std::time::Instant::now();
            for _ in 0..n {
                let img = image::RgbaImage::from_raw(f.width, f.height, f.rgba.clone()).unwrap();
                let hh = (f.height as u64 * mw as u64 / f.width as u64) as u32;
                let img =
                    image::imageops::resize(&img, mw, hh, image::imageops::FilterType::Triangle);
                let rgb: Vec<u8> = img
                    .as_raw()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect();
                let mut out = Vec::new();
                JpegEncoder::new_with_quality(&mut out, q)
                    .encode(&rgb, mw, hh, ExtendedColorType::Rgb8)
                    .unwrap();
            }
            println!(
                "{name} (old way): {:.1} ms per frame",
                t.elapsed().as_secs_f64() * 1000.0 / n as f64
            );
        }
    }
}
