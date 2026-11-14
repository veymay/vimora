//! Windows: the lock screen and the presentation drawn with plain Win32 / GDI.
//!
//! The GPU path (OpenGL / Direct3D through egui) left the screen black on some PCs — a
//! full-screen GPU window depends on the graphics driver. GDI draws on every Windows PC. The fade
//! in / out is the window's own transparency (a layered window), not a black veil.
//!
//! Lives inside `overlay` (shares its state, the stdin commands and the input hooks).

use super::{input_block, Mode, Shared};
use eframe::egui;
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vimora_ui::icons;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const FADE_IN: f32 = 0.2;
const FADE_OUT: f32 = 0.15;

fn rgb(c: egui::Color32) -> COLORREF {
    c.r() as u32 | (c.g() as u32) << 8 | (c.b() as u32) << 16
}

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The unlock code box (same rules as the egui version).
#[derive(Default)]
struct Code {
    entered: String,
    failures: u32,
    wrong_at: Option<Instant>,
    blocked_until: Option<Instant>,
    unlocked_at: Option<Instant>,
    typed_at: Option<Instant>,
}

struct State {
    shared: Arc<Mutex<Shared>>,
    started: Instant,
    code: Code,
    alpha: u8,
    /// What was drawn last (redraw only when it changes).
    drawn: String,
    /// Background for this size (gradient, glow, corners).
    background: Option<(i32, i32, HDC, HBITMAP)>,
    /// The accent the cached background was drawn with.
    bg_accent: [u8; 3],
    /// Presentation frame as BGRA, with its size.
    frame: Option<(Vec<u8>, i32, i32)>,
    scale: f32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn font(height: f32, bold: bool, face: &str) -> HFONT {
    let f = wide(face);
    unsafe {
        CreateFontW(
            -(height.round() as i32),
            0,
            0,
            0,
            if bold { 700 } else { 400 },
            0,
            0,
            0,
            DEFAULT_CHARSET as _,
            OUT_TT_PRECIS as _,
            CLIP_DEFAULT_PRECIS as _,
            CLEARTYPE_QUALITY as _,
            (DEFAULT_PITCH | FF_DONTCARE) as _,
            f.as_ptr(),
        )
    }
}

/// Text centred (horizontally) in `r`, wrapped; returns its height.
unsafe fn text(hdc: HDC, s: &str, f: HFONT, color: egui::Color32, r: RECT, flags: u32) -> i32 {
    let old = SelectObject(hdc, f as HGDIOBJ);
    SetTextColor(hdc, rgb(color));
    let mut w: Vec<u16> = s.encode_utf16().collect();
    let mut calc = r;
    DrawTextW(
        hdc,
        w.as_mut_ptr(),
        w.len() as i32,
        &mut calc,
        flags | DT_CALCRECT,
    );
    let h = calc.bottom - calc.top;
    let mut out = r;
    if flags & DT_SINGLELINE == 0 {
        out.bottom = out.top + h;
    }
    DrawTextW(hdc, w.as_mut_ptr(), w.len() as i32, &mut out, flags);
    SelectObject(hdc, old);
    h
}

unsafe fn fill(hdc: HDC, r: RECT, c: egui::Color32) {
    let b = CreateSolidBrush(rgb(c));
    FillRect(hdc, &r, b);
    DeleteObject(b as HGDIOBJ);
}

unsafe fn frame_rect(hdc: HDC, r: RECT, c: egui::Color32, width: i32) {
    for i in 0..width {
        let b = CreateSolidBrush(rgb(c));
        let rr = RECT {
            left: r.left + i,
            top: r.top + i,
            right: r.right - i,
            bottom: r.bottom - i,
        };
        FrameRect(hdc, &rr, b);
        DeleteObject(b as HGDIOBJ);
    }
}

unsafe fn circle(hdc: HDC, x: i32, y: i32, r: i32, c: egui::Color32) {
    let b = CreateSolidBrush(rgb(c));
    let ob = SelectObject(hdc, b as HGDIOBJ);
    let op = SelectObject(hdc, GetStockObject(NULL_PEN));
    Ellipse(hdc, x - r, y - r, x + r + 1, y + r + 1);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(b as HGDIOBJ);
}

/// The brand mark (from `vimora_ui::logo_shapes`) in the square starting at (x, y).
unsafe fn logo(hdc: HDC, x: i32, y: i32, size: i32) {
    let (top, bottom) = vimora_ui::LOGO_GRADIENT;
    // The gradient square, row by row.
    for row in 0..size {
        let t = row as f32 / size.max(1) as f32;
        fill(
            hdc,
            RECT {
                left: x,
                top: y + row,
                right: x + size,
                bottom: y + row + 1,
            },
            mix(top, bottom, t),
        );
    }
    let k = size as f32 / 256.0;
    for (pts, color) in vimora_ui::logo_shapes() {
        let points: Vec<POINT> = pts
            .iter()
            .map(|&(px, py)| POINT {
                x: x + (px * k).round() as i32,
                y: y + ((py + vimora_ui::LOGO_SHIFT) * k).round() as i32,
            })
            .collect();
        let b = CreateSolidBrush(rgb(color));
        let ob = SelectObject(hdc, b as HGDIOBJ);
        let op = SelectObject(hdc, GetStockObject(NULL_PEN));
        Polygon(hdc, points.as_ptr(), points.len() as i32);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        DeleteObject(b as HGDIOBJ);
    }
}

/// Gradient, soft glow behind the icon and the corner brackets, drawn once per size.
unsafe fn make_background(screen: HDC, w: i32, h: i32, s: f32, accent: [u8; 3]) -> (HDC, HBITMAP) {
    let mut p = vimora_ui::DARK;
    if accent != [0, 0, 0] {
        p.accent = egui::Color32::from_rgb(accent[0], accent[1], accent[2]);
    }
    let dc = CreateCompatibleDC(screen);
    let bmp = CreateCompatibleBitmap(screen, w, h);
    SelectObject(dc, bmp as HGDIOBJ);
    let top = egui::Color32::from_rgb(0x1d, 0x1f, 0x1b);
    let bottom = egui::Color32::from_rgb(0x0e, 0x0f, 0x0d);
    let step = 4;
    let mut y = 0;
    while y < h {
        let c = mix(top, bottom, y as f32 / h.max(1) as f32);
        fill(
            dc,
            RECT {
                left: 0,
                top: y,
                right: w,
                bottom: y + step,
            },
            c,
        );
        y += step;
    }
    // Glow: rings from wide and faint to small and brighter.
    let (cx, cy) = (w / 2, h / 2 - (70.0 * s) as i32);
    let base = mix(top, bottom, cy as f32 / h.max(1) as f32);
    let rings = 48;
    for i in 0..rings {
        let t = i as f32 / rings as f32;
        let r = (520.0 * s * (1.0 - t)).max(1.0) as i32;
        let strength = 0.30 * t.powf(1.6);
        circle(dc, cx, cy, r, mix(base, p.accent, strength));
    }
    // Corner brackets.
    let m = (28.0 * s) as i32;
    let len = (46.0 * s) as i32;
    let th = (2.0 * s).max(1.0) as i32;
    let line = mix(base, p.accent, 0.55);
    for (x, y, dx, dy) in [(0, 0, 1, 1), (w, 0, -1, 1), (0, h, 1, -1), (w, h, -1, -1)] {
        let ox = x + dx * m;
        let oy = y + dy * m;
        let hr = RECT {
            left: ox.min(ox + dx * len),
            top: oy.min(oy + dy * th),
            right: ox.max(ox + dx * len),
            bottom: oy.max(oy + dy * th),
        };
        let vr = RECT {
            left: ox.min(ox + dx * th),
            top: oy.min(oy + dy * len),
            right: ox.max(ox + dx * th),
            bottom: oy.max(oy + dy * len),
        };
        fill(dc, hr, line);
        fill(dc, vr, line);
    }
    (dc, bmp)
}

/// Feeds typed keys into the code box (from the keyboard hook).
fn handle_code(st: &mut State) {
    let keys = input_block::take_keys();
    let code = st.shared.lock().unwrap().code.clone();
    let Some((salt, hash, digits)) = code else {
        return;
    };
    let c = &mut st.code;
    if c.unlocked_at.is_some() || c.blocked_until.is_some_and(|t| Instant::now() < t) {
        return;
    }
    c.blocked_until = None;
    for k in keys {
        c.typed_at = Some(Instant::now());
        match k {
            '\u{8}' => {
                c.entered.pop();
            }
            '\u{1b}' => c.entered.clear(),
            d if d.is_ascii_digit() && c.entered.len() < digits as usize => c.entered.push(d),
            _ => {}
        }
        if c.entered.len() == digits as usize {
            if vimora_core::pin::verify(&salt, &hash, &c.entered) {
                c.unlocked_at = Some(Instant::now());
                use std::io::Write;
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "UNLOCK");
                let _ = out.flush();
            } else {
                c.failures += 1;
                c.wrong_at = Some(Instant::now());
                if c.failures.is_multiple_of(5) {
                    let secs = (30 * (c.failures as u64 / 5)).min(300);
                    c.blocked_until = Some(Instant::now() + Duration::from_secs(secs));
                }
            }
            c.entered.clear();
            break;
        }
    }
}

unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let screen = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right, rc.bottom);
    let s = st.scale;
    let p = vimora_ui::DARK;
    let dc = CreateCompatibleDC(screen);
    let bmp = CreateCompatibleBitmap(screen, w, h);
    let old_bmp = SelectObject(dc, bmp as HGDIOBJ);
    SetBkMode(dc, TRANSPARENT as _);

    let mode = st.shared.lock().unwrap().mode.clone();
    match mode {
        Mode::Show => {
            fill(dc, rc, egui::Color32::BLACK);
            if let Some((px, fw, fh)) = &st.frame {
                let k = (w as f32 / *fw as f32).min(h as f32 / *fh as f32);
                let (dw, dh) = ((*fw as f32 * k) as i32, (*fh as f32 * k) as i32);
                let mut bi: BITMAPINFO = std::mem::zeroed();
                bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bi.bmiHeader.biWidth = *fw;
                bi.bmiHeader.biHeight = -*fh; // top-down
                bi.bmiHeader.biPlanes = 1;
                bi.bmiHeader.biBitCount = 32;
                bi.bmiHeader.biCompression = BI_RGB;
                SetStretchBltMode(dc, HALFTONE);
                StretchDIBits(
                    dc,
                    (w - dw) / 2,
                    (h - dh) / 2,
                    dw,
                    dh,
                    0,
                    0,
                    *fw,
                    *fh,
                    px.as_ptr() as *const _,
                    &bi,
                    DIB_RGB_COLORS,
                    SRCCOPY,
                );
            } else {
                let f = font(26.0 * s, false, "Geist");
                text(
                    dc,
                    "O'qituvchi ekrani yuklanmoqda…",
                    f,
                    p.muted,
                    rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                DeleteObject(f as HGDIOBJ);
            }
        }
        Mode::Lock { title, message } => {
            let look = st.shared.lock().unwrap().look.clone();
            let mut p = p;
            if look.accent != [0, 0, 0] {
                p.accent = egui::Color32::from_rgb(look.accent[0], look.accent[1], look.accent[2]);
            }
            let face = if look.font == 1 {
                "Geist Mono"
            } else {
                "Geist"
            };
            if st.background.map(|b| (b.0, b.1)) != Some((w, h)) || st.bg_accent != look.accent {
                if let Some((_, _, bdc, bb)) = st.background.take() {
                    DeleteDC(bdc);
                    DeleteObject(bb as HGDIOBJ);
                }
                let (bdc, bb) = make_background(screen, w, h, s, look.accent);
                st.background = Some((w, h, bdc, bb));
                st.bg_accent = look.accent;
            }
            if let Some((_, _, bdc, _)) = st.background {
                BitBlt(dc, 0, 0, w, h, bdc, 0, 0, SRCCOPY);
            }
            let (icon_name, digits) = {
                let sh = st.shared.lock().unwrap();
                (sh.icon.clone(), sh.code.as_ref().map(|c| c.2))
            };
            let c = &st.code;
            let now = Instant::now();
            let unlocked = c.unlocked_at.is_some();
            let wrong = c
                .wrong_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(1200));
            let typing = c
                .typed_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_secs(6));
            let cx = w / 2;
            let cy = h / 2 - (70.0 * s) as i32;

            // Brand and clock.
            let small = font(15.0 * s, true, "Geist Mono");
            let brand = if look.brand.trim().is_empty() {
                "VIMORA".to_string()
            } else {
                look.brand.trim().to_uppercase()
            };
            if !look.no_logo {
                // The same mark as the egui screens (see `vimora_ui::logo_shapes`).
                logo(dc, (48.0 * s) as i32, (44.0 * s) as i32, (30.0 * s) as i32);
                text(
                    dc,
                    &brand,
                    small,
                    p.muted,
                    RECT {
                        left: (48.0 * s) as i32 + (42.0 * s) as i32,
                        top: (44.0 * s) as i32,
                        right: w / 2,
                        bottom: (74.0 * s) as i32,
                    },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            DeleteObject(small as HGDIOBJ);
            let k = match look.clock {
                1 => 0.7,
                2 => 1.5,
                _ => 1.0,
            };
            let clock = font(30.0 * s * k, true, "Geist Mono");
            let nowc = chrono::Local::now();
            if look.clock != 3 {
                text(
                    dc,
                    &nowc.format("%H:%M").to_string(),
                    clock,
                    p.text,
                    RECT {
                        left: w / 2,
                        top: (34.0 * s) as i32,
                        right: w - (48.0 * s) as i32,
                        bottom: (72.0 * s) as i32 + (30.0 * s * (k - 1.0)).max(0.0) as i32,
                    },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            DeleteObject(clock as HGDIOBJ);
            let date = font(13.0 * s * k, false, "Geist Mono");
            if look.clock != 3 {
                text(
                    dc,
                    &nowc.format("%d.%m.%Y").to_string(),
                    date,
                    p.faint,
                    RECT {
                        left: w / 2,
                        top: (72.0 * s) as i32 + (30.0 * s * (k - 1.0)).max(0.0) as i32,
                        right: w - (48.0 * s) as i32,
                        bottom: (92.0 * s) as i32 + (40.0 * s * (k - 1.0)).max(0.0) as i32,
                    },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            DeleteObject(date as HGDIOBJ);

            // The icon.
            let glyph = if unlocked {
                icons::LOCK_OPEN.to_string()
            } else {
                icon_name.unwrap_or_else(|| icons::LOCK.to_string())
            };
            let icon_font = font(84.0 * s, false, "lucide");
            let half = (70.0 * s) as i32;
            text(
                dc,
                &glyph,
                icon_font,
                if unlocked { p.success } else { p.accent },
                RECT {
                    left: cx - half * 2,
                    top: cy - half,
                    right: cx + half * 2,
                    bottom: cy + half,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            DeleteObject(icon_font as HGDIOBJ);

            // Title and message.
            let mut y = cy + (92.0 * s) as i32;
            if !title.trim().is_empty() {
                let f = font(38.0 * s, true, face);
                let r = RECT {
                    left: w / 10,
                    top: y,
                    right: w - w / 10,
                    bottom: y + 400,
                };
                y += text(dc, &title, f, p.text, r, DT_CENTER | DT_WORDBREAK) + (14.0 * s) as i32;
                DeleteObject(f as HGDIOBJ);
            }
            if !message.trim().is_empty() {
                let f = font(21.0 * s, false, face);
                let r = RECT {
                    left: w * 175 / 1000,
                    top: y,
                    right: w - w * 175 / 1000,
                    bottom: y + 400,
                };
                y +=
                    text(dc, &message, f, p.muted, r, DT_CENTER | DT_WORDBREAK) + (14.0 * s) as i32;
                DeleteObject(f as HGDIOBJ);
            }

            // Code cells.
            if let Some(n) = digits {
                y += (26.0 * s) as i32;
                let cell = (56.0 * s) as i32;
                let gap = (12.0 * s) as i32;
                let total = n as i32 * cell + (n as i32 - 1) * gap;
                let shake = c
                    .wrong_at
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .filter(|a| *a < 0.5)
                    .map(|a| ((a * 46.0).sin() * 14.0 * s * (1.0 - a / 0.5)) as i32)
                    .unwrap_or(0);
                let x0 = cx - total / 2 + shake;
                let blocked = c.blocked_until.and_then(|t| t.checked_duration_since(now));
                let edge = if unlocked {
                    p.success
                } else if wrong || blocked.is_some() {
                    p.danger
                } else if typing {
                    p.accent
                } else {
                    p.line_strong
                };
                let filled_n = if unlocked {
                    n as usize
                } else {
                    c.entered.len()
                };
                for i in 0..n as usize {
                    let r = RECT {
                        left: x0 + i as i32 * (cell + gap),
                        top: y,
                        right: x0 + i as i32 * (cell + gap) + cell,
                        bottom: y + cell,
                    };
                    let filled = i < filled_n;
                    fill(dc, r, if filled { p.sunken } else { p.card });
                    let active = i == filled_n && typing && !unlocked;
                    frame_rect(dc, r, edge, if active { (2.0 * s) as i32 } else { 1 });
                    if filled {
                        let dot = if unlocked {
                            p.success
                        } else if wrong {
                            p.danger
                        } else {
                            p.accent
                        };
                        circle(
                            dc,
                            (r.left + r.right) / 2,
                            (r.top + r.bottom) / 2,
                            (8.5 * s) as i32,
                            dot,
                        );
                    }
                }
                let (msg, col) = if unlocked {
                    ("Ochildi".to_string(), p.success)
                } else if let Some(left) = blocked {
                    (
                        format!(
                            "Juda ko'p urinish. {} soniyadan keyin qayta urining",
                            left.as_secs() + 1
                        ),
                        p.danger,
                    )
                } else if wrong {
                    ("Kod noto'g'ri".to_string(), p.danger)
                } else {
                    ("Ochish kodini kiriting".to_string(), p.faint)
                };
                let f = font(14.0 * s, false, "Geist Mono");
                let top = y + cell + (14.0 * s) as i32;
                text(
                    dc,
                    &msg,
                    f,
                    col,
                    RECT {
                        left: 0,
                        top,
                        right: w,
                        bottom: top + (28.0 * s) as i32,
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                DeleteObject(f as HGDIOBJ);
            }

            let f = font(15.0 * s, false, "Geist");
            text(
                dc,
                "Vimora",
                f,
                p.faint,
                RECT {
                    left: 0,
                    top: h - (60.0 * s) as i32,
                    right: w,
                    bottom: h - (20.0 * s) as i32,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            DeleteObject(f as HGDIOBJ);
        }
    }

    BitBlt(screen, 0, 0, w, h, dc, 0, 0, SRCCOPY);
    SelectObject(dc, old_bmp);
    DeleteObject(bmp as HGDIOBJ);
    DeleteDC(dc);
    EndPaint(hwnd, &ps);
}

/// Every 33 ms: keys, the presentation frame, transparency, and a redraw when anything shown
/// changed.
unsafe fn tick(hwnd: HWND, st: &mut State) {
    // The mouse can't move (the hook eats it), so the pointer never "enters" us: hide it here.
    SetCursor(null_mut());
    handle_code(st);
    let (closing, frame, mode_key) = {
        let mut sh = st.shared.lock().unwrap();
        let frame = if sh.frame_dirty {
            sh.frame_dirty = false;
            sh.frame.take()
        } else {
            None
        };
        let key = match &sh.mode {
            Mode::Show => "show".to_string(),
            Mode::Lock { title, message } => {
                format!(
                    "{title}|{message}|{:?}|{:?}",
                    sh.icon,
                    sh.code.as_ref().map(|c| c.2)
                )
            }
        };
        (sh.closing, frame, key)
    };
    let mut force = false;
    if let Some(img) = frame {
        let px: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| [c.b(), c.g(), c.r(), 255])
            .collect();
        st.frame = Some((px, img.size[0] as i32, img.size[1] as i32));
        force = true;
    }
    // Transparency: fade in from nothing, fade out to nothing, then leave.
    let fin = (st.started.elapsed().as_secs_f32() / FADE_IN).min(1.0);
    let fin = 1.0 - (1.0 - fin).powi(3);
    let fout = closing
        .map(|t| (t.elapsed().as_secs_f32() / FADE_OUT).min(1.0))
        .unwrap_or(0.0);
    if fout >= 1.0 {
        std::process::exit(0);
    }
    let alpha = (255.0 * fin * (1.0 - fout)) as u8;
    if alpha != st.alpha {
        st.alpha = alpha;
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
    let now = Instant::now();
    let c = &st.code;
    let shaking = c
        .wrong_at
        .is_some_and(|t| now.duration_since(t) < Duration::from_millis(600));
    let key = format!(
        "{mode_key}|{}|{}|{}|{}|{:?}|{}",
        c.entered.len(),
        c.unlocked_at.is_some(),
        c.wrong_at
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(1200)),
        c.typed_at
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(6)),
        c.blocked_until
            .and_then(|t| t.checked_duration_since(now))
            .map(|d| d.as_secs()),
        chrono::Local::now().format("%H:%M"),
    );
    if force || shaking || key != st.drawn {
        st.drawn = key;
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    paint(hwnd, st);
                }
            });
            0
        }
        WM_TIMER => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    tick(hwnd, st);
                }
            });
            0
        }
        WM_ERASEBKGND => 1,
        // No pointer over the lock screen.
        WM_SETCURSOR => {
            SetCursor(null_mut());
            1
        }
        // Never closed or minimized by the student (the agent ends the process).
        WM_CLOSE | WM_SYSCOMMAND
            if msg == WM_CLOSE
                || (w & 0xFFF0) == SC_CLOSE as usize
                || (w & 0xFFF0) == SC_MINIMIZE as usize =>
        {
            0
        }
        // "Show desktop" (Win+D, a touchpad swipe) and the like try to hide or minimize every
        // window: this one stays where it is.
        WM_WINDOWPOSCHANGING => {
            let wp = &mut *(l as *mut WINDOWPOS);
            wp.flags &= !SWP_HIDEWINDOW;
            if wp.x <= -30000 || wp.y <= -30000 {
                wp.flags |= SWP_NOMOVE | SWP_NOSIZE;
            }
            0
        }
        WM_SIZE if w as u32 == SIZE_MINIMIZED => {
            PostMessageW(hwnd, WM_SYSCOMMAND, SC_RESTORE as usize, 0);
            0
        }
        WM_DPICHANGED => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.scale = (w & 0xFFFF) as f32 / 96.0;
                    st.background = None;
                }
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// Loads the fonts the screen uses (Geist, Geist Mono, the icon font) for this process only.
unsafe fn load_fonts() {
    for data in [
        vimora_ui::font_files::REGULAR,
        vimora_ui::font_files::BOLD,
        vimora_ui::font_files::MONO_BOLD,
        vimora_ui::font_files::ICONS,
    ] {
        let n = 0u32;
        AddFontMemResourceEx(data.as_ptr() as *const _, data.len() as u32, null(), &n);
    }
}

pub(super) fn run(shared: Arc<Mutex<Shared>>) -> anyhow::Result<()> {
    unsafe {
        crate::system::set_dpi_aware();
        load_fonts();
        let module = GetModuleHandleW(null());
        let class = wide("VimoraLock");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let (cx, cy) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let title = wide("Vimora");
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            cx,
            // One pixel taller than the screen: never treated as exclusive full screen.
            cy + 1,
            null_mut(),
            null_mut(),
            module,
            null(),
        );
        if hwnd.is_null() {
            anyhow::bail!("lock window could not be created");
        }
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                shared,
                started: Instant::now(),
                code: Code::default(),
                alpha: 0,
                drawn: String::new(),
                background: None,
                bg_accent: [0; 3],
                frame: None,
                scale: dpi as f32 / 96.0,
            })
        });
        // Fully transparent first; the timer fades it in.
        SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetTimer(hwnd, 1, 33, None);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
