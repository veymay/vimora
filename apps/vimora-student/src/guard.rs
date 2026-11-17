//! The student can't switch Vimora off.
//!
//! * [`protect_self`]: the agent and its windows give ordinary users no right to end, suspend
//!   or change them — Task Manager's "End task" and `taskkill` get "Access denied", also for
//!   administrators. Only SYSTEM keeps every right: updates and the teacher's "remove" go
//!   through the service.
//! * [`keep_agent_running`]: the service (SYSTEM) starts the agent again in the signed-in
//!   user's session within seconds if it is gone anyway (an administrator ended it, a crash).
//!
//! Windows only; elsewhere both do nothing.

/// The port the watcher listens on (only to show it is alive).
fn guard_port() -> u16 {
    crate::service::port().wrapping_add(1)
}

fn alive(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

/// Starts `<this exe> <arg>` detached (out of the caller's job when it may).
#[cfg(windows)]
fn spawn_detached(arg: &str) {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    const NO_WINDOW: u32 = 0x0800_0000;
    const BREAKAWAY: u32 = 0x0100_0000;
    const DETACHED: u32 = 0x0000_0008;
    for flags in [NO_WINDOW | DETACHED | BREAKAWAY, NO_WINDOW | DETACHED] {
        if std::process::Command::new(&exe)
            .arg(arg)
            .creation_flags(flags)
            .spawn()
            .is_ok()
        {
            return;
        }
    }
}

/// `vimora-student guard`: the service's watcher (Windows). Holds a port so there is only one,
/// and starts the service again at once when its port stops answering.
pub fn run_guard() -> anyhow::Result<()> {
    let Ok(lock) = std::net::TcpListener::bind(("127.0.0.1", guard_port())) else {
        return Ok(()); // another watcher is running
    };
    // The service only knocks to see that this port answers: the knocks are taken and dropped
    // (unaccepted ones would fill the queue and the port would look dead).
    std::thread::spawn(move || for _ in lock.incoming() {});
    // A new version was installed: this copy leaves, the new service starts a new watcher.
    crate::selfcheck::watch("guard", |_exe| std::process::exit(0));
    let mut down = 0u32;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if alive(crate::service::port()) {
            down = 0;
            continue;
        }
        down += 1;
        if down >= 2 {
            down = 0;
            #[cfg(windows)]
            spawn_detached("service");
        }
    }
}

/// Called by the service: makes sure the watcher exists (starts it when its port is silent).
#[cfg(windows)]
pub fn keep_guard_running() {
    let _ = std::thread::Builder::new()
        .name("keep-guard".into())
        .spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if !alive(guard_port()) {
                spawn_detached("guard");
            }
        });
}

#[cfg(windows)]
mod imp {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo, SDDL_REVISION_1,
        SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, ACL, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    use windows_sys::Win32::System::RemoteDesktop::{
        ProcessIdToSessionId, WTSGetActiveConsoleSessionId, WTSQueryUserToken,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetCurrentProcess, CREATE_BREAKAWAY_FROM_JOB,
        CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// SYSTEM (the service, which updates and removes Vimora): everything. Everyone else,
    /// administrators too: only look at the process (query limited information, synchronize,
    /// read its permissions) — no terminate, no suspend. The owner (the signed-in user) gets
    /// read-only rights as well, so the permissions cannot be rewritten by hand.
    const SDDL: &str = "D:P(A;;GA;;;SY)(A;;0x121000;;;WD)(A;;0x20000;;;OW)";

    pub fn protect_self() {
        unsafe {
            let sddl = wide(SDDL);
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            ) == 0
            {
                tracing::warn!("guard: cannot build the security descriptor");
                return;
            }
            let (mut present, mut defaulted) = (0, 0);
            let mut dacl: *mut ACL = null_mut();
            if GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) != 0
                && present != 0
            {
                let r = SetSecurityInfo(
                    GetCurrentProcess(),
                    SE_KERNEL_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    dacl,
                    null(),
                );
                if r != 0 {
                    tracing::warn!("guard: SetSecurityInfo failed ({r})");
                }
            }
            LocalFree(sd as _);
        }
    }

    /// Is a Vimora agent (or one of its windows) running in `session`?
    unsafe fn agent_in(session: u32) -> bool {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return true; // can't tell: don't start a second one
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            // "vimora-client.exe": 0.3.27 and older, or a frozen PC not moved yet.
            if name == "vimora-student.exe" || name == "vimora-client.exe" {
                let mut sid = u32::MAX;
                if ProcessIdToSessionId(e.th32ProcessID, &mut sid) != 0 && sid == session {
                    found = true;
                    break;
                }
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        found
    }

    /// Starts `"<this exe>" run` as the user signed in to `session`, on their desktop.
    unsafe fn launch(session: u32) -> bool {
        let mut token: HANDLE = null_mut();
        if WTSQueryUserToken(session, &mut token) == 0 {
            return false; // nobody signed in
        }
        let Ok(exe) = std::env::current_exe() else {
            CloseHandle(token);
            return false;
        };
        let dir = exe
            .parent()
            .map(|d| wide(&d.to_string_lossy()))
            .unwrap_or_else(|| wide("C:\\"));
        let mut cmd = wide(&format!("\"{}\" run --restarted", exe.display()));
        let mut env = null_mut();
        let have_env = CreateEnvironmentBlock(&mut env, token, 0) != 0;
        let mut desk = wide("winsta0\\default");
        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        si.lpDesktop = desk.as_mut_ptr();
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let mut started = false;
        // Out of the service's job first (the agent must outlive a service restart); some jobs
        // forbid that, then without.
        for flags in [
            CREATE_UNICODE_ENVIRONMENT | CREATE_BREAKAWAY_FROM_JOB,
            CREATE_UNICODE_ENVIRONMENT,
        ] {
            if CreateProcessAsUserW(
                token,
                null(),
                cmd.as_mut_ptr(),
                null(),
                null(),
                0,
                flags,
                if have_env { env } else { null_mut() },
                dir.as_ptr(),
                &si,
                &mut pi,
            ) != 0
            {
                started = true;
                CloseHandle(pi.hThread);
                CloseHandle(pi.hProcess);
                break;
            }
        }
        if have_env {
            DestroyEnvironmentBlock(env);
        }
        CloseHandle(token);
        started
    }

    pub fn keep_agent_running() {
        let _ = std::thread::Builder::new()
            .name("keep-agent".into())
            .spawn(|| {
                // Give the logon start (Run key) its chance first.
                std::thread::sleep(std::time::Duration::from_secs(10));
                let mut missing = 0u32;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    unsafe {
                        let session = WTSGetActiveConsoleSessionId();
                        if session == u32::MAX || session == 0 {
                            missing = 0;
                            continue;
                        }
                        if agent_in(session) {
                            missing = 0;
                            continue;
                        }
                        // Gone for two checks in a row (not just restarting itself).
                        missing += 1;
                        if missing < 2 {
                            continue;
                        }
                        missing = 0;
                        if launch(session) {
                            tracing::info!("agent was not running in session {session}: started");
                        }
                    }
                }
            });
    }
}

#[cfg(windows)]
pub use imp::{keep_agent_running, protect_self};

#[cfg(not(windows))]
pub fn protect_self() {}

#[cfg(not(windows))]
pub fn keep_agent_running() {}
