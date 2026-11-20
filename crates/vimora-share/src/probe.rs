//! What this computer can really do, found by trying (never by "the package is installed").
//!
//! The encoder choice rests on a **real encode test**: each candidate encodes a short test
//! stream and is timed. Hardware is used only when its test passed, it is fast enough with
//! margin, and its rate control can hold a bitrate (a Wi-Fi link needs that). Otherwise the
//! software encoder is the reliable fallback.

use serde::Serialize;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Runs a program with a time limit; `None` if it could not start or ran too long (it is killed).
fn run(prog: &str, args: &[&str], limit: Duration) -> Option<(bool, String, String)> {
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let out = child.wait_with_output().ok()?;
                return Some((
                    status.success(),
                    String::from_utf8_lossy(&out.stdout).into_owned(),
                    String::from_utf8_lossy(&out.stderr).into_owned(),
                ));
            }
            None if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SystemCaps {
    /// "wayland", "x11" or "".
    pub session_type: String,
    pub desktop: String,
    pub render_nodes: Vec<String>,
    pub gstreamer: bool,
    /// Elements the pipelines may use, and whether each exists.
    pub elements: BTreeMap<String, bool>,
    /// Video encode entrypoints the VA-API driver lists (e.g. "H264Main:EncSliceLP").
    pub vaapi_encode: Vec<String>,
    pub vaapi_driver: String,
}

const ELEMENTS: &[&str] = &[
    "pipewiresrc",
    "ximagesrc",
    "vapostproc",
    "vah264enc",
    "vah264lpenc",
    "x264enc",
    "openh264enc",
    "h264parse",
    "mpegtsmux",
    "rtph264pay",
];

/// Looks at the session, the GPU nodes, GStreamer and VA-API. Fast (no encoding).
pub fn detect() -> SystemCaps {
    let mut c = SystemCaps {
        session_type: std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        ..Default::default()
    };
    if let Ok(rd) = std::fs::read_dir("/dev/dri") {
        c.render_nodes = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("renderD"))
            .collect();
        c.render_nodes.sort();
    }
    c.gstreamer = run("gst-inspect-1.0", &["--version"], Duration::from_secs(5)).is_some();
    if c.gstreamer {
        for e in ELEMENTS {
            let ok = run("gst-inspect-1.0", &[e], Duration::from_secs(10)).is_some_and(|r| r.0);
            c.elements.insert((*e).into(), ok);
        }
    }
    if let Some((_, out, err)) = run("vainfo", &[], Duration::from_secs(10)) {
        let text = format!("{out}\n{err}");
        c.vaapi_driver = text
            .lines()
            .find_map(|l| {
                l.split_once("Driver version:")
                    .map(|x| x.1.trim().to_string())
            })
            .unwrap_or_default();
        c.vaapi_encode = parse_vainfo_encode(&text);
    }
    c
}

/// `VAProfileH264Main : VAEntrypointEncSliceLP` → "H264Main:EncSliceLP" (encode entrypoints only).
pub fn parse_vainfo_encode(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let (p, e) = l.split_once(':')?;
            let p = p.trim().strip_prefix("VAProfile")?;
            let e = e.trim().strip_prefix("VAEntrypoint")?;
            (e.starts_with("Enc")).then(|| format!("{p}:{e}"))
        })
        .collect()
}

/// A candidate encoder.
#[derive(Debug, Clone, Copy)]
pub struct EncoderSpec {
    pub name: &'static str,
    pub hardware: bool,
    /// The pipeline part from raw video to H.264 (with `{kbps}` for the bitrate).
    pub pipeline: &'static str,
    /// Software encoders hold a bitrate by design; for hardware it is read from the element.
    pub holds_bitrate: bool,
}

pub const CANDIDATES: &[EncoderSpec] = &[
    EncoderSpec {
        name: "vah264enc",
        hardware: true,
        pipeline: "vapostproc ! video/x-raw(memory:VAMemory),format=NV12 ! vah264enc rate-control=cbr bitrate={kbps} key-int-max=60",
        holds_bitrate: false,
    },
    EncoderSpec {
        name: "vah264lpenc",
        hardware: true,
        pipeline: "vapostproc ! video/x-raw(memory:VAMemory),format=NV12 ! vah264lpenc bitrate={kbps} key-int-max=60",
        holds_bitrate: false,
    },
    EncoderSpec {
        name: "x264enc",
        hardware: false,
        pipeline: "x264enc tune=zerolatency speed-preset=ultrafast bitrate={kbps} key-int-max=60",
        holds_bitrate: true,
    },
    EncoderSpec {
        name: "openh264enc",
        hardware: false,
        pipeline: "openh264enc bitrate={bps} gop-size=60",
        holds_bitrate: true,
    },
];

/// The rate-control modes an element offers (`"cbr"`, `"vbr"`, `"cqp"` …), from its description.
pub fn rate_control_modes(inspect_text: &str) -> Vec<String> {
    let mut modes = vec![];
    let mut inside = false;
    for l in inspect_text.lines() {
        let t = l.trim();
        if t.starts_with("rate-control") {
            inside = true;
            continue;
        }
        if inside {
            if let Some(rest) = t.strip_prefix('(') {
                // "(16): cqp              - Constant Quantizer"
                if let Some((_, after)) = rest.split_once("):") {
                    if let Some(name) = after.split_whitespace().next() {
                        modes.push(name.to_string());
                    }
                }
            } else if !t.is_empty() && !t.starts_with("flags") && !t.starts_with("Enum") {
                break;
            }
        }
    }
    modes
}

#[derive(Debug, Clone, Serialize)]
pub struct EncodeTest {
    pub encoder: String,
    pub hardware: bool,
    /// The test stream encoded without an error.
    pub ok: bool,
    pub fps: f64,
    pub seconds: f64,
    pub width: u32,
    pub height: u32,
    pub rate_control: Vec<String>,
    /// Can this encoder hold a bitrate?
    pub holds_bitrate: bool,
    pub error: Option<String>,
}

/// Encodes `frames` test frames and times it. Real work, so it takes a few seconds.
pub fn run_encode_test(
    spec: &EncoderSpec,
    width: u32,
    height: u32,
    fps: u32,
    frames: u32,
) -> EncodeTest {
    let mut t = EncodeTest {
        encoder: spec.name.into(),
        hardware: spec.hardware,
        ok: false,
        fps: 0.0,
        seconds: 0.0,
        width,
        height,
        rate_control: vec![],
        holds_bitrate: spec.holds_bitrate,
        error: None,
    };
    let Some((exists, text, _)) = run("gst-inspect-1.0", &[spec.name], Duration::from_secs(10))
    else {
        t.error = Some("gst-inspect did not run".into());
        return t;
    };
    if !exists {
        t.error = Some("the encoder element is not installed".into());
        return t;
    }
    t.rate_control = rate_control_modes(&text);
    if spec.hardware {
        t.holds_bitrate = t.rate_control.iter().any(|m| m == "cbr" || m == "vbr");
    }
    let kbps = if height >= 1080 { 8000 } else { 4000 };
    let enc = spec
        .pipeline
        .replace("{kbps}", &kbps.to_string())
        .replace("{bps}", &(kbps * 1000).to_string());
    // Hardware elements that do not offer cbr must not be given it.
    let enc = if spec.hardware && !t.holds_bitrate {
        enc.replace("rate-control=cbr ", "")
    } else {
        enc
    };
    let launch = format!(
        "videotestsrc num-buffers={frames} pattern=ball is-live=false ! \
         video/x-raw,width={width},height={height},framerate={fps}/1 ! videoconvert ! {enc} ! \
         h264parse ! fakesink sync=false"
    );
    let args: Vec<&str> = std::iter::once("-q")
        .chain(launch.split_whitespace())
        .collect();
    let start = Instant::now();
    match run("gst-launch-1.0", &args, Duration::from_secs(60)) {
        None => t.error = Some("the encode test did not finish in 60 s".into()),
        Some((ok, _, err)) => {
            t.seconds = start.elapsed().as_secs_f64();
            if ok && !err.contains("ERROR") && !err.contains("erroneous") {
                t.ok = true;
                t.fps = frames as f64 / t.seconds.max(0.001);
            } else {
                t.error = Some(err.lines().next().unwrap_or("failed").trim().to_string());
            }
        }
    }
    t
}

/// Picks the encoder: a hardware one that passed, is fast enough (×1.3 margin) and can hold a
/// bitrate; otherwise the fastest software one that passed. `None` if nothing works.
pub fn choose(tests: &[EncodeTest], target_fps: f64) -> Option<&EncodeTest> {
    let fit = |t: &&EncodeTest| t.ok && t.holds_bitrate && t.fps >= target_fps * 1.3;
    tests
        .iter()
        .filter(fit)
        .filter(|t| t.hardware)
        .max_by(|a, b| a.fps.total_cmp(&b.fps))
        .or_else(|| {
            // Software: prefer the one with the best margin that passed.
            tests
                .iter()
                .filter(|t| t.ok && !t.hardware && t.holds_bitrate)
                .filter(|t| t.fps >= target_fps)
                .max_by(|a, b| a.fps.total_cmp(&b.fps))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, hw: bool, ok: bool, fps: f64, cbr: bool) -> EncodeTest {
        EncodeTest {
            encoder: name.into(),
            hardware: hw,
            ok,
            fps,
            seconds: 1.0,
            width: 1920,
            height: 1080,
            rate_control: vec![],
            holds_bitrate: cbr,
            error: None,
        }
    }

    #[test]
    fn hardware_wins_only_when_tested_fast_and_able_to_hold_a_bitrate() {
        let tests = [
            t("vah264lpenc", true, true, 200.0, false), // fast, but CQP only
            t("x264enc", false, true, 40.0, true),
            t("openh264enc", false, true, 73.0, true),
        ];
        // Hardware cannot hold a bitrate here: the fastest software encoder is chosen.
        assert_eq!(choose(&tests, 30.0).unwrap().encoder, "openh264enc");
        let tests = [
            t("vah264enc", true, true, 120.0, true),
            t("x264enc", false, true, 40.0, true),
        ];
        assert_eq!(choose(&tests, 30.0).unwrap().encoder, "vah264enc");
    }

    #[test]
    fn a_failed_or_too_slow_hardware_encoder_is_never_chosen() {
        let tests = [
            t("vah264enc", true, false, 0.0, true),
            t("vah264lpenc", true, true, 35.0, true), // below 30 × 1.3
            t("x264enc", false, true, 45.0, true),
        ];
        assert_eq!(choose(&tests, 30.0).unwrap().encoder, "x264enc");
    }

    #[test]
    fn nothing_that_works_gives_none() {
        let tests = [t("x264enc", false, true, 10.0, true)];
        assert!(choose(&tests, 30.0).is_none()); // too slow for 30 fps
        assert!(choose(&[], 30.0).is_none());
    }

    #[test]
    fn vainfo_lists_only_encode_entrypoints() {
        let text = "      VAProfileH264Main               :\tVAEntrypointVLD\n\
                    \x20     VAProfileH264Main               :\tVAEntrypointEncSliceLP\n\
                    \x20     VAProfileNone                   :\tVAEntrypointVideoProc\n\
                    \x20     VAProfileHEVCMain               :\tVAEntrypointEncSliceLP\n";
        assert_eq!(
            parse_vainfo_encode(text),
            ["H264Main:EncSliceLP", "HEVCMain:EncSliceLP"]
        );
    }

    #[test]
    fn rate_control_modes_come_from_the_element_description() {
        let text = "  rate-control        : The desired rate control mode\n\
                    \x20                       flags: readable, writable\n\
                    \x20                       Enum \"X\" Default: 16, \"cqp\"\n\
                    \x20                          (16): cqp              - Constant Quantizer\n\
                    \x20                          (4): cbr               - Constant Bitrate\n\
                    \x20 ref-frames          : Number of reference frames\n";
        assert_eq!(rate_control_modes(text), ["cqp", "cbr"]);
    }

    /// Real: needs GStreamer. `cargo test -p vimora-share -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_encode_test_of_the_software_encoder() {
        let r = run_encode_test(&CANDIDATES[2], 1280, 720, 30, 60);
        println!("{r:?}");
        assert!(r.ok, "{:?}", r.error);
        assert!(r.fps > 5.0);
    }
}
