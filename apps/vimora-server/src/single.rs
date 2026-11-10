//! One console per computer. A second start (desktop shortcut, menu, autostart) doesn't open a
//! second console — it asks the running one to show its window and exits.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// Local-only port the running console listens on for "show yourself".
const PORT: u16 = 47803;

/// `Some(listener)`: we are the first console. `None`: another one is running and was asked
/// to come to the front (the caller should exit).
pub fn claim() -> Option<TcpListener> {
    if std::env::var_os("VIMORA_SCREENSHOT").is_some() || std::env::var_os("VIMORA_MULTI").is_some()
    {
        // Dev runs next to a live console.
        return TcpListener::bind(("127.0.0.1", 0)).ok();
    }
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Some(l),
        Err(_) => {
            if let Ok(mut s) =
                TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_secs(2))
            {
                let _ = s.write_all(b"show");
            }
            None
        }
    }
}

/// Brings the window back whenever another start knocks.
pub fn serve(listener: TcpListener, ctx: eframe::egui::Context) {
    use eframe::egui::ViewportCommand as V;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4];
            let mut s = stream;
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            if s.read_exact(&mut buf).is_ok() && &buf == b"show" {
                ctx.send_viewport_cmd(V::Visible(true));
                ctx.send_viewport_cmd(V::Minimized(false));
                ctx.send_viewport_cmd(V::Focus);
                ctx.request_repaint();
            }
        }
    });
}
