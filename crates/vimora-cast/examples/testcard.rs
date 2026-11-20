//! `cargo run -p vimora-cast --example testcard -- 192.168.1.43 15` — shows a colour-bar test card
//! with a moving square on a Cast display for N seconds (not the desktop), then stops.
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vimora_cast::{Device, Session, State};

fn card(n: u32) -> Vec<u8> {
    let (w, h) = (1280u32, 720u32);
    let mut img = image::RgbImage::new(w, h);
    let bars = [
        [255, 255, 255],
        [255, 255, 0],
        [0, 255, 255],
        [0, 255, 0],
        [255, 0, 255],
        [255, 0, 0],
        [0, 0, 255],
        [20, 20, 20],
    ];
    for (x, _, px) in img.enumerate_pixels_mut() {
        *px = image::Rgb(bars[(x * 8 / w) as usize]);
    }
    // A square that moves one step per picture: motion tells if the display keeps up.
    let sx = (n * 40) % (w - 100);
    for y in 300..400 {
        for x in sx..sx + 100 {
            img.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&img)
        .unwrap();
    out
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut a = std::env::args().skip(1);
    let ip: std::net::Ipv4Addr = a
        .next()
        .expect("usage: testcard <ip> [seconds]")
        .parse()
        .unwrap();
    let secs: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(15);
    let dev = Device {
        id: ip.to_string(),
        name: ip.to_string(),
        model: String::new(),
        addr: ip,
        port: 8009,
    };
    let n = Arc::new(AtomicU32::new(0));
    let src = {
        let n = n.clone();
        Arc::new(move || Some(card(n.fetch_add(1, Ordering::Relaxed))))
    };
    let s = Session::start(&tokio::runtime::Handle::current(), dev, src);
    let t0 = Instant::now();
    let mut last = None;
    while t0.elapsed() < Duration::from_secs(secs) {
        let st = s.state();
        if last.as_ref() != Some(&st) {
            println!(
                "{:>5.1}s {:?} (pictures made: {})",
                t0.elapsed().as_secs_f32(),
                st,
                n.load(Ordering::Relaxed)
            );
            last = Some(st.clone());
        }
        if matches!(st, State::Failed(_) | State::Stopped) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!("pictures made in total: {}", n.load(Ordering::Relaxed));
    println!("display stats: {:?}", s.stats());
    s.stop();
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("final: {:?}", s.state());
}
