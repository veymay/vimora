//! `cargo run -p vimora-cast --example probe -- 192.168.1.43` — read-only look at a Cast display.
use vimora_cast::{diag, Device};

#[tokio::main]
async fn main() {
    let ip: std::net::Ipv4Addr = std::env::args()
        .nth(1)
        .expect("usage: probe <ip>")
        .parse()
        .expect("an IPv4 address");
    let dev = Device {
        id: ip.to_string(),
        name: ip.to_string(),
        model: String::new(),
        addr: ip,
        port: 8009,
    };
    println!("{:#?}", diag::probe(&dev).await);
}
