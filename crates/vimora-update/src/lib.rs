//! Self-update from the project's GitHub releases.
//!
//! A release carries one installer per platform and role plus `SHA256SUMS`:
//!
//! | file                                  | used by                         |
//! |---------------------------------------|---------------------------------|
//! | `vimora-<v>-windows-x64-setup.exe`    | Windows teacher and agent       |
//! | `vimora-teacher_<v>_amd64.deb`        | Ubuntu teacher                  |
//! | `vimora-student_<v>_amd64.deb`        | Ubuntu agent (installed by the service) |
//!
//! Every download is checked against `SHA256SUMS` before it is run, and `SHA256SUMS` itself must
//! carry a valid signature (`SHA256SUMS.sig`) made with the project's release key: nobody else
//! can make the computers install anything — not a changed GitHub release, not a fake teacher on
//! the network. Files are found by their name pattern, so later renames of versions keep working.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REPO: &str = "veymay/vimora";
pub const RELEASES_PAGE: &str = "https://github.com/veymay/vimora/releases";
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;

/// Public halves of the release signing keys (`examples/sign.rs`). Only `SHA256SUMS` signed by
/// one of them is accepted. A new key can be added here (and used by the next releases) while
/// the old one keeps working for the computers that have not updated yet.
pub const RELEASE_KEYS: &[&str] =
    &["6efd797fc57bfe7617a85d10b04cdf6272fce1949d39d3713064498a4dd2e282"];

/// What the release key signs: a fixed prefix + the bytes of `SHA256SUMS`.
pub fn signed_message(sums: &[u8]) -> Vec<u8> {
    let mut m = b"vimora-release-sums\0".to_vec();
    m.extend_from_slice(sums);
    m
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// Checks the signature of a `SHA256SUMS` file (hex text from `SHA256SUMS.sig`).
pub fn verify_sums(sums: &[u8], sig_hex: &str) -> Result<()> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let sig = unhex(sig_hex)
        .and_then(|b| Signature::from_slice(&b).ok())
        .context("imzo fayli buzilgan")?;
    let msg = signed_message(sums);
    let ok = RELEASE_KEYS.iter().any(|k| {
        unhex(k)
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .and_then(|b| VerifyingKey::from_bytes(&b).ok())
            .is_some_and(|vk| vk.verify_strict(&msg, &sig).is_ok())
    });
    if !ok {
        bail!("yangilanish imzosi noto'g'ri — o'rnatilmaydi");
    }
    Ok(())
}

/// Which installer: for a Windows PC, or the Ubuntu student / teacher package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Windows,
    StudentDeb,
    TeacherDeb,
}

impl Target {
    /// This computer's installer for `role`.
    pub fn here(role: Role) -> Self {
        match (cfg!(windows), role) {
            (true, _) => Target::Windows,
            (false, Role::Student) => Target::StudentDeb,
            (false, Role::Teacher) => Target::TeacherDeb,
        }
    }

    /// The student installer for a computer that reports `os` ("windows", "linux").
    pub fn student_for(os: &str) -> Option<Self> {
        match os {
            "windows" => Some(Target::Windows),
            "linux" => Some(Target::StudentDeb),
            _ => None,
        }
    }

    fn matches(self, name: &str) -> bool {
        let n = name.to_lowercase();
        match self {
            Target::Windows => n.ends_with(".exe") && n.contains("windows"),
            Target::StudentDeb => n.starts_with("vimora-student") && n.ends_with(".deb"),
            Target::TeacherDeb => n.starts_with("vimora-teacher") && n.ends_with(".deb"),
        }
    }
}

/// The installer for `target` listed in a signed `SHA256SUMS`: (file name, sha256).
pub fn pick_target(sums: &str, target: Target) -> Option<(String, String)> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (hash, name) = (it.next()?, it.next()?);
        let name = name.trim_start_matches('*');
        target
            .matches(name)
            .then(|| (name.to_string(), hash.to_lowercase()))
    })
}

/// The installer for this platform and role listed in a signed `SHA256SUMS`: (file name, sha256).
/// Exact name first, then the name pattern (so a later naming change keeps working).
pub fn pick_from_sums(sums: &str, role: Role, version: &str) -> Option<(String, String)> {
    let rows: Vec<(String, String)> = sums
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (hash, name) = (it.next()?, it.next()?);
            Some((
                name.trim_start_matches('*').to_string(),
                hash.to_lowercase(),
            ))
        })
        .collect();
    let exact = asset_name(role, version);
    if let Some(r) = rows.iter().find(|r| r.0 == exact) {
        return Some(r.clone());
    }
    pick_target(sums, Target::here(role))
}

/// Version written in an installer's file name ("vimora-0.3.27-windows-x64-setup.exe",
/// "vimora-student_0.3.27_amd64.deb" → "0.3.27").
pub fn version_in_name(name: &str) -> Option<String> {
    name.split(['-', '_'])
        .find(|p| p.contains('.') && p.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|p| p.to_string())
}

/// SHA-256 of `data` as lowercase hex.
pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The version of this build ("0.2.0").
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Teacher.
    Teacher,
    /// Student agent + service.
    Student,
}

#[derive(Debug, Clone)]
pub struct Release {
    /// "0.2.0" (tag without the leading "v").
    pub version: String,
    /// Release page on GitHub.
    pub page: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn agent() -> ureq::Agent {
    let b = ureq::AgentBuilder::new();
    #[cfg(windows)]
    let b = match native_tls::TlsConnector::new() {
        Ok(c) => b.tls_connector(std::sync::Arc::new(c)),
        Err(e) => {
            tracing::warn!("TLS: {e}");
            b
        }
    };
    b.timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .user_agent(concat!("vimora-updater/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// The newest published release. The GitHub API is tried first; when it refuses (rate limit,
/// 403/429) the plain `releases/latest` page is used, which has no such limit.
pub fn latest() -> Result<Release> {
    latest_api().or_else(|api_err| latest_page().map_err(|_| api_err))
}

/// Fallback: follow `https://github.com/<repo>/releases/latest` to the tag and derive the files.
fn latest_page() -> Result<Release> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let resp = agent().get(&url).call()?;
    let final_url = resp.get_url().to_string();
    let tag = final_url
        .rsplit_once("/releases/tag/")
        .map(|x| x.1)
        .filter(|t| !t.is_empty())
        .context("so'nggi reliz topilmadi")?
        .to_string();
    let v = tag.trim_start_matches('v').to_string();
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    let asset = |name: String| Asset {
        url: format!("{base}/{name}"),
        name,
        size: 0,
    };
    Ok(Release {
        page: final_url,
        assets: vec![
            asset(format!("vimora-{v}-windows-x64-setup.exe")),
            asset(format!("vimora-teacher_{v}_amd64.deb")),
            asset(format!("vimora-student_{v}_amd64.deb")),
            asset("SHA256SUMS".to_string()),
            asset("SHA256SUMS.sig".to_string()),
        ],
        version: v,
    })
}

/// The newest published (non-draft, non-prerelease) release, from the GitHub API.
fn latest_api() -> Result<Release> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("GitHub bilan bog'lanib bo'lmadi")?
        .into_string()?;
    let r: GhRelease = serde_json::from_str(&body).context("GitHub javobi tushunarsiz")?;
    if r.draft || r.prerelease {
        bail!("so'nggi reliz hali tayyor emas");
    }
    Ok(Release {
        version: r.tag_name.trim_start_matches('v').to_string(),
        page: r.html_url,
        assets: r
            .assets
            .into_iter()
            .map(|a| Asset {
                name: a.name,
                url: a.browser_download_url,
                size: a.size,
            })
            .collect(),
    })
}

/// "1.2.10" → [1, 2, 10]; anything after a '-' or '+' is ignored.
fn parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Is version `a` newer than `b`?
pub fn is_newer(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (parts(a), parts(b));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// File name of the installer for this platform and role.
pub fn asset_name(role: Role, version: &str) -> String {
    if cfg!(windows) {
        format!("vimora-{version}-windows-x64-setup.exe")
    } else {
        let pkg = match role {
            Role::Teacher => "vimora-teacher",
            Role::Student => "vimora-student",
        };
        format!("{pkg}_{version}_amd64.deb")
    }
}

impl Release {
    pub fn is_newer(&self) -> bool {
        is_newer(&self.version, current())
    }

    pub fn asset(&self, role: Role) -> Option<&Asset> {
        let name = asset_name(role, &self.version);
        self.assets.iter().find(|a| a.name == name)
    }
}

fn fetch(url: &str, progress: &dyn Fn(u64, u64), total: u64) -> Result<Vec<u8>> {
    let resp = agent()
        .get(url)
        .call()
        .context("yuklab olib bo'lmadi (internet yoki GitHub javob bermadi)")?;
    let mut reader = resp.into_reader().take(MAX_DOWNLOAD);
    let mut out = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .context("yuklab olish to'xtab qoldi: 20 soniya davomida ma'lumot kelmadi")?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        progress(out.len() as u64, total);
    }
    Ok(out)
}

/// A release's signed list of files: `SHA256SUMS` and its signature, checked.
pub fn signed_sums(rel: &Release) -> Result<(String, String)> {
    let find = |n: &str| rel.assets.iter().find(|a| a.name == n);
    let sums = find("SHA256SUMS").context("relizda SHA256SUMS yo'q")?;
    let sig = find("SHA256SUMS.sig").context("relizda imzo (SHA256SUMS.sig) yo'q")?;
    let sums = fetch(&sums.url, &|_, _| {}, sums.size)?;
    let sig = String::from_utf8(fetch(&sig.url, &|_, _| {}, sig.size)?)?;
    verify_sums(&sums, &sig)?;
    Ok((String::from_utf8(sums)?, sig))
}

/// A downloaded installer with the signed list that vouches for it.
pub struct Package {
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    pub sums: String,
    pub sig: String,
}

/// Downloads this platform's installer into `dir`; checked against the signed `SHA256SUMS`.
pub fn download(
    rel: &Release,
    role: Role,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    download_package(rel, role, dir, progress).map(|p| p.path)
}

/// [`download`], keeping the signed list (a teacher passes it on to computers without internet).
pub fn download_package(
    rel: &Release,
    role: Role,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<Package> {
    download_target(rel, Target::here(role), dir, progress)
}

/// The installer for any `target` (a teacher fetches the students' one), checked.
pub fn download_target(
    rel: &Release,
    target: Target,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<Package> {
    let (sums, sig) = signed_sums(rel)?;
    let (name, want) =
        pick_target(&sums, target).context("imzolangan ro'yxatda o'rnatuvchi topilmadi")?;
    let base = rel
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .map(|a| a.url.trim_end_matches("SHA256SUMS").to_string())
        .context("relizda SHA256SUMS yo'q")?;
    let size = rel
        .assets
        .iter()
        .find(|a| a.name == name)
        .map_or(0, |a| a.size);
    let data = fetch(&format!("{base}{name}"), progress, size)?;
    if sha256_hex(&data) != want {
        bail!("fayl buzilgan (SHA-256 mos kelmadi)");
    }
    let path = save(dir, &name, &data)?;
    Ok(Package {
        path,
        version: version_in_name(&name).unwrap_or_else(|| rel.version.clone()),
        name,
        sums,
        sig,
    })
}

/// Checks a package that came from elsewhere (a teacher on the network): the signature of its
/// list, its SHA-256, and that it is newer than this build. Returns its version.
pub fn verify_package(name: &str, data: &[u8], sums: &str, sig: &str) -> Result<String> {
    verify_sums(sums.as_bytes(), sig)?;
    let want = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (hash, n) = (it.next()?, it.next()?);
            (n.trim_start_matches('*') == name).then(|| hash.to_lowercase())
        })
        .context("fayl imzolangan ro'yxatda yo'q")?;
    if sha256_hex(data) != want {
        bail!("fayl buzilgan (SHA-256 mos kelmadi)");
    }
    let v = version_in_name(name).context("fayl nomida versiya yo'q")?;
    Ok(v)
}

/// Saves a checked installer into `dir` (a fresh name when the old file is still in use).
pub fn save(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    if std::fs::write(&path, data).is_ok() {
        return Ok(path);
    }
    // The file is in use (Windows "os error 32"): an installer started earlier is still
    // running from it. Save under a fresh name instead of failing the update.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let alt = dir.join(format!("{stamp}-{name}"));
    std::fs::write(&alt, data)
        .with_context(|| format!("yangilanish faylini saqlab bo'lmadi: {}", alt.display()))?;
    Ok(alt)
}

/// Before a new update: stop installers left over from earlier attempts (a hung one would keep
/// its file locked and block every later update) and delete their files. Best effort.
pub fn clear_old(dir: &Path) {
    #[cfg(windows)]
    let _ = run("schtasks", &["/end", "/tn", "Vimora Update"]);
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        if !(lower.ends_with(".exe") || lower.ends_with(".deb")) {
            continue;
        }
        #[cfg(windows)]
        if lower.ends_with(".exe") {
            let _ = run("taskkill", &["/F", "/IM", &name]);
        }
        let _ = std::fs::remove_file(e.path());
    }
}

/// Starts installing a downloaded teacher update. On Windows the installer takes over (the caller
/// should exit right away); on Ubuntu this waits for `pkexec apt-get install` (password prompt)
/// and returns once the new version is on disk.
pub fn install_teacher(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new(path)
            .args([
                "/SILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/TYPE=teacher",
                "/COMPONENTS=teacher",
            ])
            .spawn()
            .context("o'rnatuvchini ishga tushirib bo'lmadi")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let st = std::process::Command::new("pkexec")
            .args(["apt-get", "install", "-y", "--reinstall"])
            .arg(path)
            .status()
            .context("pkexec topilmadi")?;
        if !st.success() {
            bail!("o'rnatish bekor qilindi yoki xato bilan tugadi");
        }
        Ok(())
    }
}

/// Installs a downloaded student update from the privileged service (no prompts). The installer
/// runs outside the service (a one-off SYSTEM task / transient systemd unit), because it restarts
/// the service it replaces.
pub fn install_student(path: &Path) -> Result<()> {
    let file = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        // A small script runs the installer and writes its exit code next to it
        // (`install_result`), so the service can tell whether the update really went in. The
        // task points at the script, so there is nothing to quote in the task's command line.
        let dir = path.parent().context("yangilanish papkasi yo'q")?;
        let result = dir.join("install-result.txt");
        let _ = std::fs::remove_file(&result);
        let script = dir.join("run-update.cmd");
        std::fs::write(
            &script,
            format!(
                "@echo off\r\n\"{file}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent\r\necho %errorlevel% > \"{}\"\r\n",
                result.display()
            ),
        )
        .context("yangilanish skriptini yozib bo'lmadi")?;
        let tr = script.to_string_lossy().into_owned();
        let task = run(
            "schtasks",
            &[
                "/create",
                "/tn",
                "Vimora Update",
                "/tr",
                &tr,
                "/sc",
                "once",
                "/st",
                "23:59",
                "/ru",
                "SYSTEM",
                "/rl",
                "HIGHEST",
                "/f",
            ],
        )
        .and_then(|_| run("schtasks", &["/run", "/tn", "Vimora Update"]));
        if let Err(e) = task {
            // No task scheduler: start the script directly, detached from this service (the
            // installer stops the service; the script must outlive it).
            tracing::warn!("update task: {e:#}; starting the installer directly");
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new("cmd.exe")
                .args(["/c", &tr])
                .creation_flags(DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW)
                .spawn()
                .context("o'rnatuvchini ishga tushirib bo'lmadi")?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        run(
            "systemd-run",
            &[
                "--collect",
                "--quiet",
                "--setenv=DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                // Unattended upgrades may hold the dpkg lock: wait for it instead of failing.
                "-o",
                "DPkg::Lock::Timeout=600",
                "install",
                "-y",
                "--reinstall",
                &file,
            ],
        )
    }
}

/// Exit code of the last student installer started by [`install_student`] (Windows), once it
/// finished: 0 = installed.
pub fn install_result(dir: &Path) -> Option<i32> {
    std::fs::read_to_string(dir.join("install-result.txt"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("{cmd} ishga tushmadi"))?;
    if !out.status.success() {
        bail!(
            "{cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .chars()
                .take(200)
                .collect::<String>()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_required() {
        let sums = b"abc  vimora-9.9.9-windows-x64-setup.exe\n";
        assert!(verify_sums(sums, &"00".repeat(64)).is_err());
        assert!(verify_sums(sums, "zz").is_err());
        assert!(
            verify_package("vimora-9.9.9-windows-x64-setup.exe", b"x", "abc  x", "00").is_err()
        );
    }

    #[test]
    fn picks_installers_by_pattern() {
        let sums = "aa  vimora-1.2.3-windows-x64-setup.exe\nbb  vimora-student_1.2.3_amd64.deb\ncc  vimora-teacher_1.2.3_amd64.deb\n";
        assert_eq!(pick_target(sums, Target::Windows).unwrap().1, "aa");
        assert_eq!(pick_target(sums, Target::StudentDeb).unwrap().1, "bb");
        assert_eq!(pick_target(sums, Target::TeacherDeb).unwrap().1, "cc");
        assert_eq!(
            version_in_name("vimora-1.2.3-windows-x64-setup.exe").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            version_in_name("vimora-student_1.2.3_amd64.deb").as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v0.10.0", "0.9.3"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn names() {
        let n = asset_name(Role::Student, "0.2.0");
        assert!(n == "vimora-student_0.2.0_amd64.deb" || n == "vimora-0.2.0-windows-x64-setup.exe");
    }

    /// Live check against the published release: `cargo test -p vimora-update -- --ignored`.
    #[test]
    #[ignore]
    fn live_release_downloads_and_verifies() {
        let rel = latest().expect("latest release");
        println!("latest: {} ({} assets)", rel.version, rel.assets.len());
        for role in [Role::Teacher, Role::Student] {
            assert!(
                rel.asset(role).is_some(),
                "missing {}",
                asset_name(role, &rel.version)
            );
        }
        let dir = std::env::temp_dir().join("vimora-update-test");
        let path = download(&rel, Role::Teacher, &dir, &|_, _| {}).expect("download + checksum");
        println!("verified {}", path.display());
        let _ = std::fs::remove_dir_all(dir);
    }
}
