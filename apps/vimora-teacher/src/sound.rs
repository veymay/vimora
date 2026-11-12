//! Short notification sounds, synthesised at start-up (no audio files, no licences to track).
//!
//! * [`Sound::Ok`] — a command succeeded (two soft rising notes);
//! * [`Sound::Warn`] — something failed (two low notes);
//! * [`Sound::Alert`] — a student needs attention: blocked search, help request (three notes).

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Ok,
    Warn,
    Alert,
}

const RATE: u32 = 44_100;

/// One bell-like note: a sine with a soft attack, an exponential decay and a quiet overtone.
fn note(out: &mut Vec<i16>, freq: f32, secs: f32, volume: f32) {
    let n = (RATE as f32 * secs) as usize;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let attack = (t / 0.006).min(1.0);
        let env = attack * (-t * 9.0).exp();
        let w = std::f32::consts::TAU * freq * t;
        let v = (w.sin() + 0.25 * (2.0 * w).sin()) * env * volume;
        out.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
}

fn wav(samples: &[i16]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// The WAV bytes of a sound (built once).
pub fn bytes(s: Sound) -> &'static [u8] {
    static OK: OnceLock<Vec<u8>> = OnceLock::new();
    static WARN: OnceLock<Vec<u8>> = OnceLock::new();
    static ALERT: OnceLock<Vec<u8>> = OnceLock::new();
    let build = |notes: &[(f32, f32)]| {
        let mut pcm = Vec::new();
        for (f, d) in notes {
            note(&mut pcm, *f, *d, 0.45);
        }
        wav(&pcm)
    };
    match s {
        Sound::Ok => OK.get_or_init(|| build(&[(880.0, 0.10), (1318.5, 0.22)])),
        Sound::Warn => WARN.get_or_init(|| build(&[(392.0, 0.14), (311.1, 0.30)])),
        Sound::Alert => ALERT.get_or_init(|| build(&[(987.8, 0.11), (784.0, 0.11), (987.8, 0.26)])),
    }
}

/// Plays a sound without blocking (silently does nothing when no audio is available).
pub fn play(s: Sound) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        // SND_MEMORY: the "name" is the WAV image itself; it is 'static, so SND_ASYNC is safe.
        unsafe {
            PlaySoundW(
                bytes(s).as_ptr() as *const u16,
                std::ptr::null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
    #[cfg(not(windows))]
    {
        std::thread::spawn(move || {
            let name = match s {
                Sound::Ok => "ok",
                Sound::Warn => "warn",
                Sound::Alert => "alert",
            };
            let dir = std::env::temp_dir().join("vimora-sounds");
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!("{name}.wav"));
            if !file.exists() && std::fs::write(&file, bytes(s)).is_err() {
                return;
            }
            for player in ["pw-play", "paplay", "aplay"] {
                let mut cmd = std::process::Command::new(player);
                if player == "aplay" {
                    cmd.arg("-q");
                }
                let done = cmd
                    .arg(&file)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                if done.is_ok_and(|st| st.success()) {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_is_well_formed() {
        for s in [Sound::Ok, Sound::Warn, Sound::Alert] {
            let b = bytes(s);
            assert_eq!(&b[..4], b"RIFF");
            assert_eq!(&b[8..12], b"WAVE");
            let data = u32::from_le_bytes(b[40..44].try_into().unwrap()) as usize;
            assert_eq!(b.len(), 44 + data);
            assert!(data > 10_000);
        }
    }
}
