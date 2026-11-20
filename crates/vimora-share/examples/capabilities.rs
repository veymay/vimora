//! `cargo run -p vimora-share --example capabilities` — what this computer can do, as JSON:
//! the session, GPU nodes, GStreamer elements, VA-API entrypoints, a real encode test of every
//! candidate encoder (720p30 and 1080p30) and the encoder that would be chosen.
use vimora_share::probe::*;

fn main() {
    let caps = detect();
    let mut tests = vec![];
    for (w, h) in [(1280, 720), (1920, 1080)] {
        for spec in CANDIDATES {
            tests.push(run_encode_test(spec, w, h, 30, 300));
        }
    }
    let chosen_1080 = choose(
        &tests
            .iter()
            .filter(|t| t.height == 1080)
            .cloned()
            .collect::<Vec<_>>(),
        30.0,
    )
    .map(|t| t.encoder.clone());
    let report = serde_json::json!({
        "system": caps,
        "encode_tests": tests,
        "chosen_for_1080p30": chosen_1080,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
