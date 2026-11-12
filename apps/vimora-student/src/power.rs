//! Keeps the PC reachable: while the Vimora service runs the computer does not fall asleep on
//! its own (idle timeout), so the teacher can always reach the agent. Closing a laptop lid or
//! pressing the sleep button still sleeps; the agent reconnects by itself after waking up.

/// Called once by the service. Lives as long as the service does.
pub fn keep_awake() {
    #[cfg(windows)]
    {
        let _ = std::thread::Builder::new()
            .name("keep-awake".into())
            .spawn(|| {
                use windows_sys::Win32::System::Power::{
                    SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
                };
                loop {
                    // The state lasts while this thread lives; refreshed in case it was reset.
                    unsafe {
                        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
                    }
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            });
    }
    #[cfg(not(windows))]
    {
        // An inhibitor lock held by a child process; it goes away with the service.
        let _ = std::process::Command::new("systemd-inhibit")
            .args([
                "--what=sleep:idle",
                "--who=Vimora",
                "--why=Classroom agent must stay reachable",
                "--mode=block",
                "sleep",
                "infinity",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}
