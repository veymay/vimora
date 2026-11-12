//! Sound on/off for the device policy ("Ovoz"). Runs in the user's session (the agent): the
//! volume belongs to the logged-in user. While sound is off the agent keeps it muted (a student
//! who unmutes is muted again within seconds).

use std::sync::atomic::{AtomicBool, Ordering};

/// Sound is turned off by the teacher.
static OFF: AtomicBool = AtomicBool::new(false);
/// The lock screen is up: silent, like a frozen computer.
static LOCKED: AtomicBool = AtomicBool::new(false);

/// Lock screen shown (true) or gone: mute while it is up, sound back afterwards (unless the
/// teacher turned sound off).
pub fn set_locked(on: bool) {
    let was = LOCKED.swap(on, Ordering::Relaxed);
    if was == on {
        return;
    }
    if on {
        mute(true);
    } else if !OFF.load(Ordering::Relaxed) {
        mute(false);
    }
}

fn flag_file() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("sound-off"))
}

/// Remembered across agent restarts.
pub fn load() {
    if flag_file().is_some_and(|f| f.exists()) {
        OFF.store(true, Ordering::Relaxed);
    }
}

pub fn set_off(off: bool) {
    let was = OFF.swap(off, Ordering::Relaxed);
    if let Some(f) = flag_file() {
        if off {
            let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
            let _ = std::fs::write(&f, "1");
        } else {
            let _ = std::fs::remove_file(&f);
        }
    }
    if off || was {
        mute(off || LOCKED.load(Ordering::Relaxed));
    }
}

/// Called every few seconds: re-mutes while sound is off.
pub fn enforce() {
    if OFF.load(Ordering::Relaxed) || LOCKED.load(Ordering::Relaxed) {
        mute(true);
    }
}

#[cfg(windows)]
fn mute(on: bool) {
    // Core Audio through PowerShell (the default playback device's endpoint volume).
    const SCRIPT: &str = r#"
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
[Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioEndpointVolume { int f(); int g(); int h(); int i(); int SetMasterVolumeLevelScalar(float l, System.Guid c); int j(); int GetMasterVolumeLevelScalar(out float l); int k(); int l(); int m(); int n(); int SetMute([MarshalAs(UnmanagedType.Bool)] bool b, System.Guid c); int GetMute(out bool b); }
[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice { int Activate(ref System.Guid id, int ctx, int p, out IAudioEndpointVolume v); }
[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator { int f(); int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice d); }
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorComObject { }
public class VimoraAudio {
  public static void Mute(bool on) {
    var e = new MMDeviceEnumeratorComObject() as IMMDeviceEnumerator;
    IMMDevice d; Marshal.ThrowExceptionForHR(e.GetDefaultAudioEndpoint(0, 1, out d));
    IAudioEndpointVolume v; var id = typeof(IAudioEndpointVolume).GUID;
    Marshal.ThrowExceptionForHR(d.Activate(ref id, 23, 0, out v));
    bool m; v.GetMute(out m); if (m != on) { v.SetMute(on, System.Guid.Empty); }
  }
}
'@
"#;
    let code = format!(
        "{SCRIPT}[VimoraAudio]::Mute(${})",
        if on { "true" } else { "false" }
    );
    if let Err(e) = crate::sh::script(&code) {
        tracing::debug!("mute: {e:#}");
    }
}

#[cfg(not(windows))]
fn mute(on: bool) {
    let v = if on { "1" } else { "0" };
    let ok = crate::sh::run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", v])
        .map(|(ok, _)| ok)
        .unwrap_or(false);
    if !ok {
        let _ = crate::sh::run(
            "amixer",
            &["-q", "set", "Master", if on { "mute" } else { "unmute" }],
        );
    }
}
