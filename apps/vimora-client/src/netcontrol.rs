//! NetControl: internet restrictions on the student PC.
//!
//! Three layers, all browser-agnostic where possible:
//! 1. **hosts file** — blocked domains resolve to 0.0.0.0; with SafeSearch on, search engines
//!    are pinned to their official "safe" addresses (forcesafesearch.google.com, strict.bing.com,
//!    restrict.youtube.com, safe.duckduckgo.com, familysearch.yandex.ru).
//! 2. **Chromium-family policies** (Chrome, Edge, Brave, Yandex, Chromium) — `URLBlocklist`,
//!    `ForceGoogleSafeSearch`, `ForceYouTubeRestrict` and DNS-over-HTTPS off, so browsers can't
//!    bypass the hosts file.
//! 3. **Keyword watcher** — the title of the focused browser window carries the search query
//!    ("casino - Google Search", "casino — Яндекс"); if it contains a blocked keyword the tab is
//!    closed with Ctrl+W and the teacher is alerted.
//!
//! Layers 1 and 2 need write access to the hosts file / policy locations; the installer grants it.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::Command;
use vimora_core::NetPolicy;

const BEGIN: &str = "# >>> vimora netcontrol (managed automatically) >>>";
const END: &str = "# <<< vimora netcontrol <<<";

/// (safe endpoint, hostnames pinned to it)
const SAFE_SEARCH: &[(&str, &[&str])] = &[
    (
        "forcesafesearch.google.com",
        &[
            "google.com",
            "www.google.com",
            "google.ru",
            "www.google.ru",
            "google.co.uz",
            "www.google.co.uz",
        ],
    ),
    ("strict.bing.com", &["bing.com", "www.bing.com"]),
    (
        "safe.duckduckgo.com",
        &["duckduckgo.com", "www.duckduckgo.com"],
    ),
    (
        "restrict.youtube.com",
        &[
            "www.youtube.com",
            "m.youtube.com",
            "youtubei.googleapis.com",
            "youtube.googleapis.com",
            "www.youtube-nocookie.com",
        ],
    ),
    (
        "familysearch.yandex.ru",
        &[
            "yandex.ru",
            "www.yandex.ru",
            "yandex.uz",
            "www.yandex.uz",
            "yandex.com",
            "www.yandex.com",
            "ya.ru",
            "www.ya.ru",
        ],
    ),
];

#[derive(Default)]
pub struct NetControl {
    policy: NetPolicy,
    safe_ips: HashMap<&'static str, String>,
    /// The block we last wrote into the hosts file (re-applied if someone removes it).
    block: String,
    /// "Faqat ruxsat etilgan saytlar": browsers open only these (empty = off).
    allow: Vec<String>,
}

pub fn clean_domain(d: &str) -> Option<String> {
    let d = d.trim().to_lowercase();
    let d = d.split("://").last().unwrap_or(&d);
    let d = d
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches("*.")
        .trim_start_matches("www.")
        .to_string();
    (!d.is_empty()
        && d.contains('.')
        && d.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
    .then_some(d)
}

/// Lower-cases and unifies apostrophes so "o‘zbek", "oʻzbek" and "o'zbek" match.
pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'ʻ' | 'ʼ' | '‘' | '’' | '`' | '´' => '\'',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl NetControl {
    /// Applies a new policy. Returns a human summary for the teacher.
    pub fn apply(&mut self, mut policy: NetPolicy) -> Result<String> {
        policy.domains = policy
            .domains
            .iter()
            .filter_map(|d| clean_domain(d))
            .collect();
        policy.domains.sort();
        policy.domains.dedup();
        policy.keywords = policy
            .keywords
            .iter()
            .map(|k| normalize(k))
            .filter(|k| !k.is_empty())
            .collect();
        self.policy = policy;

        if self.policy.safe_search {
            for (endpoint, _) in SAFE_SEARCH {
                if !self.safe_ips.contains_key(endpoint) {
                    match (*endpoint, 443)
                        .to_socket_addrs()
                        .map(|mut a| a.find(|a| a.is_ipv4()))
                    {
                        Ok(Some(a)) => {
                            self.safe_ips.insert(endpoint, a.ip().to_string());
                        }
                        _ => tracing::warn!("cannot resolve {endpoint}; SafeSearch for it skipped"),
                    }
                }
            }
        }

        self.block = self.render_block();
        let hosts = write_hosts(&self.block);
        let browsers = set_browser_policies(&self.policy, &self.allow);
        flush_dns();

        let p = &self.policy;
        let mut parts = vec![];
        if !p.domains.is_empty() {
            parts.push(format!("{} ta domen", p.domains.len()));
        }
        if !p.keywords.is_empty() {
            parts.push(format!("{} ta kalit so'z", p.keywords.len()));
        }
        if p.safe_search {
            parts.push("SafeSearch".into());
        }
        let summary = if parts.is_empty() {
            "NetControl o'chirildi".to_string()
        } else {
            format!("NetControl: {}", parts.join(", "))
        };
        match (hosts, browsers) {
            (Ok(()), _) => Ok(summary),
            (Err(e), Ok(n)) if n > 0 => {
                Ok(format!("{summary} (faqat brauzer siyosati; hosts: {e:#})"))
            }
            (Err(e), _) if !p.keywords.is_empty() && p.domains.is_empty() && !p.safe_search => {
                let _ = e;
                Ok(summary)
            }
            (Err(e), _) => Err(e),
        }
    }

    /// Browsers open only these sites (empty = no such limit). Re-writes the policies.
    pub fn set_allow(&mut self, sites: &[String]) -> Result<String> {
        let mut allow: Vec<String> = sites.iter().filter_map(|d| clean_domain(d)).collect();
        allow.sort();
        allow.dedup();
        self.allow = allow;
        set_browser_policies(&self.policy, &self.allow)?;
        Ok(if self.allow.is_empty() {
            "Saytlar: oq ro'yxat o'chirildi".into()
        } else {
            format!("Saytlar: faqat {} ta saytga ruxsat", self.allow.len())
        })
    }

    /// Files / registry values the current policy wrote (for Shadow-mode commits).
    pub fn touched(&self) -> crate::sh::Touched {
        let mut t = crate::sh::Touched {
            files: vec![hosts_path()],
            registry: vec![],
        };
        #[cfg(not(windows))]
        t.files.extend(
            DIRS.iter()
                .map(|d| PathBuf::from(d).join("vimora.json"))
                .filter(|p| p.exists()),
        );
        #[cfg(windows)]
        for key in KEYS {
            for v in [
                "DnsOverHttpsMode",
                "ForceGoogleSafeSearch",
                "ForceYouTubeRestrict",
            ] {
                t.registry.push((key.to_string(), v.to_string()));
            }
            for i in 1..=self.policy.domains.len() {
                t.registry
                    .push((format!(r"{key}\URLBlocklist"), i.to_string()));
            }
            if !self.allow.is_empty() {
                t.registry
                    .push((format!(r"{key}\URLBlocklist"), "1000".to_string()));
            }
            for i in 1..=self.allow.len() {
                t.registry
                    .push((format!(r"{key}\URLAllowlist"), i.to_string()));
            }
        }
        t
    }

    /// Re-writes the hosts block if it was removed or edited.
    pub fn enforce(&mut self) {
        if self.block.is_empty() {
            return;
        }
        let current = std::fs::read_to_string(hosts_path()).unwrap_or_default();
        if !current.contains(self.block.trim()) && write_hosts(&self.block).is_ok() {
            tracing::info!("hosts block restored");
            flush_dns();
        }
    }

    fn render_block(&self) -> String {
        let p = &self.policy;
        if p.domains.is_empty() && !p.safe_search {
            return String::new();
        }
        let mut out = format!("{BEGIN}\n");
        let mut blocked = std::collections::HashSet::new();
        for d in &p.domains {
            for h in [d.clone(), format!("www.{d}"), format!("m.{d}")] {
                out.push_str(&format!("0.0.0.0 {h}\n"));
                blocked.insert(h);
            }
        }
        if p.safe_search {
            for (endpoint, hosts) in SAFE_SEARCH {
                let Some(ip) = self.safe_ips.get(endpoint) else {
                    continue;
                };
                for h in *hosts {
                    if !blocked.contains(*h) {
                        out.push_str(&format!("{ip} {h}\n"));
                    }
                }
            }
        }
        out.push_str(END);
        out.push('\n');
        out
    }
}

/// Chromium-family managed policy folders (Linux).
#[cfg(not(windows))]
const DIRS: &[&str] = &[
    "/etc/opt/chrome/policies/managed",
    "/etc/chromium/policies/managed",
    "/etc/opt/edge/policies/managed",
    "/etc/brave/policies/managed",
    "/etc/opt/yandex/browser/policies/managed",
];

/// Chromium-family policy registry keys (Windows).
#[cfg(windows)]
const KEYS: &[&str] = &[
    r"HKLM\SOFTWARE\Policies\Google\Chrome",
    r"HKLM\SOFTWARE\Policies\Microsoft\Edge",
    r"HKLM\SOFTWARE\Policies\BraveSoftware\Brave",
    r"HKLM\SOFTWARE\Policies\YandexBrowser",
    r"HKLM\SOFTWARE\Policies\Chromium",
];

fn hosts_path() -> PathBuf {
    // Test/override hook.
    if let Some(p) = std::env::var_os("VIMORA_HOSTS") {
        return PathBuf::from(p);
    }
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        PathBuf::from(root).join(r"System32\drivers\etc\hosts")
    }
    #[cfg(not(windows))]
    PathBuf::from("/etc/hosts")
}

/// Replaces our managed block in the hosts file (an empty block removes it).
fn write_hosts(block: &str) -> Result<()> {
    let path = hosts_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out = String::new();
    let mut skipping = false;
    for line in current.lines() {
        match line.trim() {
            l if l == BEGIN => skipping = true,
            l if l == END => skipping = false,
            _ if !skipping => {
                out.push_str(line);
                out.push('\n');
            }
            _ => {}
        }
    }
    out.push_str(block);
    if out == current {
        return Ok(());
    }
    // Write in place (keeps the file's permissions/ACL that the installer granted).
    std::fs::write(&path, out).with_context(|| {
        format!(
            "{} ga yozib bo'lmadi (installer huquq bermagan?)",
            path.display()
        )
    })
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn flush_dns() {
    #[cfg(windows)]
    let _ = quiet(Command::new("ipconfig").arg("/flushdns")).output();
    #[cfg(not(windows))]
    let _ = quiet(Command::new("resolvectl").arg("flush-caches")).output();
}

/// Writes Chromium-family enterprise policies. Returns how many browsers were configured.
#[cfg(not(windows))]
fn set_browser_policies(p: &NetPolicy, allow: &[String]) -> Result<usize> {
    let mut n = 0;
    for d in DIRS {
        let file = PathBuf::from(d).join("vimora.json");
        if p.is_empty() && allow.is_empty() {
            let _ = std::fs::remove_file(&file);
            continue;
        }
        let _ = std::fs::create_dir_all(d);
        // Allow-list mode: everything blocked except the listed sites.
        let mut block = p.domains.clone();
        if !allow.is_empty() {
            block.push("*".into());
        }
        let json = serde_json::json!({
            "URLBlocklist": block,
            "URLAllowlist": allow,
            "ForceGoogleSafeSearch": p.safe_search,
            "ForceYouTubeRestrict": if p.safe_search { 1 } else { 0 },
            "DnsOverHttpsMode": "off",
        });
        if std::fs::write(&file, serde_json::to_string_pretty(&json)?).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(windows)]
fn set_browser_policies(p: &NetPolicy, allow: &[String]) -> Result<usize> {
    let reg = |args: &[&str]| {
        quiet(Command::new("reg").args(args))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let mut n = 0;
    for key in KEYS {
        let list = format!(r"{key}\URLBlocklist");
        let allow_key = format!(r"{key}\URLAllowlist");
        reg(&["delete", &list, "/f"]);
        reg(&["delete", &allow_key, "/f"]);
        // Allow-list mode: everything blocked except the listed sites.
        for (i, d) in allow.iter().enumerate() {
            reg(&[
                "add",
                &allow_key,
                "/v",
                &(i + 1).to_string(),
                "/t",
                "REG_SZ",
                "/d",
                d,
                "/f",
            ]);
        }
        if !allow.is_empty() {
            reg(&["add", &list, "/v", "1000", "/t", "REG_SZ", "/d", "*", "/f"]);
        }
        if p.is_empty() {
            for v in [
                "ForceGoogleSafeSearch",
                "ForceYouTubeRestrict",
                "DnsOverHttpsMode",
            ] {
                reg(&["delete", key, "/v", v, "/f"]);
            }
            continue;
        }
        let mut ok = reg(&[
            "add",
            key,
            "/v",
            "DnsOverHttpsMode",
            "/t",
            "REG_SZ",
            "/d",
            "off",
            "/f",
        ]);
        let safe = if p.safe_search { "1" } else { "0" };
        ok &= reg(&[
            "add",
            key,
            "/v",
            "ForceGoogleSafeSearch",
            "/t",
            "REG_DWORD",
            "/d",
            safe,
            "/f",
        ]);
        ok &= reg(&[
            "add",
            key,
            "/v",
            "ForceYouTubeRestrict",
            "/t",
            "REG_DWORD",
            "/d",
            safe,
            "/f",
        ]);
        for (i, d) in p.domains.iter().enumerate() {
            ok &= reg(&[
                "add",
                &list,
                "/v",
                &(i + 1).to_string(),
                "/t",
                "REG_SZ",
                "/d",
                d,
                "/f",
            ]);
        }
        if ok {
            n += 1;
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------------------------------------
// Focused window (title + application) for the keyword watcher.

pub struct Focused {
    pub title: String,
    pub app: String,
}

const BROWSERS: &[&str] = &[
    "chrome", "chromium", "firefox", "msedge", "edge", "opera", "brave", "yandex", "browser",
    "vivaldi", "iexplore", "tor", "safari",
];

impl Focused {
    pub fn is_browser(&self) -> bool {
        let a = self.app.to_lowercase();
        BROWSERS.iter().any(|b| a.contains(b))
    }
}

pub use imp::FocusWatcher;

#[cfg(windows)]
mod imp {
    use super::Focused;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    pub struct FocusWatcher {
        sys: sysinfo::System,
    }

    impl FocusWatcher {
        pub fn new() -> anyhow::Result<Self> {
            Ok(Self {
                sys: sysinfo::System::new(),
            })
        }

        pub fn focused(&mut self) -> Option<Focused> {
            unsafe {
                let hwnd = GetForegroundWindow();
                if hwnd.is_null() {
                    return None;
                }
                let mut buf = [0u16; 512];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                let spid = sysinfo::Pid::from_u32(pid);
                self.sys
                    .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
                let app = self
                    .sys
                    .process(spid)
                    .map(|p| p.name().to_string_lossy().into_owned())
                    .unwrap_or_default();
                Some(Focused { title, app })
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::Focused;
    use anyhow::Context;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
    use x11rb::rust_connection::RustConnection;

    pub struct FocusWatcher {
        conn: RustConnection,
        root: Window,
        active: Atom,
        net_name: Atom,
        utf8: Atom,
    }

    impl FocusWatcher {
        pub fn new() -> anyhow::Result<Self> {
            let (conn, n) = x11rb::connect(None).context("no X11 display")?;
            let root = conn.setup().roots[n].root;
            let atom = |name: &[u8]| -> anyhow::Result<Atom> {
                Ok(conn.intern_atom(false, name)?.reply()?.atom)
            };
            let (active, net_name, utf8) = (
                atom(b"_NET_ACTIVE_WINDOW")?,
                atom(b"_NET_WM_NAME")?,
                atom(b"UTF8_STRING")?,
            );
            Ok(Self {
                conn,
                root,
                active,
                net_name,
                utf8,
            })
        }

        fn prop(&self, win: Window, prop: Atom, ty: Atom) -> Option<Vec<u8>> {
            let r = self
                .conn
                .get_property(false, win, prop, ty, 0, 2048)
                .ok()?
                .reply()
                .ok()?;
            (!r.value.is_empty()).then_some(r.value)
        }

        pub fn focused(&mut self) -> Option<Focused> {
            let r = self
                .conn
                .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            let win = r.value32()?.next()?;
            if win == 0 {
                return None;
            }
            let title = self
                .prop(win, self.net_name, self.utf8)
                .or_else(|| self.prop(win, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
                .map(|v| String::from_utf8_lossy(&v).into_owned())
                .unwrap_or_default();
            let app = self
                .prop(win, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())
                .map(|v| String::from_utf8_lossy(&v).replace('\0', " "))
                .unwrap_or_default();
            Some(Focused { title, app })
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Focused;
    pub struct FocusWatcher;
    impl FocusWatcher {
        pub fn new() -> anyhow::Result<Self> {
            anyhow::bail!("not supported on macOS yet")
        }
        pub fn focused(&mut self) -> Option<Focused> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        assert_eq!(
            clean_domain("https://www.YouTube.com/watch?v=1").as_deref(),
            Some("youtube.com")
        );
        assert_eq!(clean_domain("*.tiktok.com").as_deref(), Some("tiktok.com"));
        assert_eq!(clean_domain("bad domain"), None);
        assert_eq!(clean_domain("localhost"), None);
    }

    #[test]
    fn hosts_block_roundtrip() {
        let dir = std::env::temp_dir().join(format!("vimora-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hosts = dir.join("hosts");
        std::fs::write(&hosts, "127.0.0.1 localhost\n").unwrap();
        std::env::set_var("VIMORA_HOSTS", &hosts);

        let mut nc = NetControl::default();
        nc.safe_ips
            .insert("forcesafesearch.google.com", "216.239.38.120".into());
        nc.policy = NetPolicy {
            domains: vec!["youtube.com".into()],
            keywords: vec![],
            safe_search: true,
        };
        nc.block = nc.render_block();
        write_hosts(&nc.block).unwrap();
        let s = std::fs::read_to_string(&hosts).unwrap();
        assert!(s.starts_with("127.0.0.1 localhost\n"));
        assert!(s.contains("0.0.0.0 www.youtube.com"));
        assert!(s.contains("216.239.38.120 www.google.com"));

        // Student deletes our block -> enforce() restores it.
        std::fs::write(&hosts, "127.0.0.1 localhost\n").unwrap();
        nc.enforce();
        assert!(std::fs::read_to_string(&hosts)
            .unwrap()
            .contains("0.0.0.0 youtube.com"));

        // Clearing removes it again and keeps the user's lines.
        write_hosts("").unwrap();
        assert_eq!(
            std::fs::read_to_string(&hosts).unwrap(),
            "127.0.0.1 localhost\n"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn normalizing() {
        assert_eq!(normalize("  O‘ZBEK   tili "), "o'zbek tili");
        assert!(normalize("Kazino Онлайн - Google Search").contains(&normalize("онлайн")));
    }
}
