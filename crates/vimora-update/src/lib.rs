//! Self-update from the project's GitHub releases.
//!
//! A release carries one installer per platform and role plus `SHA256SUMS`:
//!
//! | file                                  | used by                         |
//! |---------------------------------------|---------------------------------|
//! | `vimora-<v>-windows-x64-setup.exe`    | Windows console and agent       |
//! | `vimora-teacher_<v>_amd64.deb`        | Ubuntu console                  |
//! | `vimora-student_<v>_amd64.deb`        | Ubuntu agent (installed by the service) |
//!
//! Every download is checked against `SHA256SUMS` before it is run.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REPO: &str = "veymay/vimora";
pub const RELEASES_PAGE: &str = "https://github.com/veymay/vimora/releases";
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;

/// The version of this build ("0.2.0").
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Teacher console.
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
        .timeout_read(Duration::from_secs(60))
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
    let resp = agent().get(url).call().context("yuklab olib bo'lmadi")?;
    let mut reader = resp.into_reader().take(MAX_DOWNLOAD);
    let mut out = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        progress(out.len() as u64, total);
    }
    Ok(out)
}

/// Downloads this platform's installer into `dir` and checks it against `SHA256SUMS`.
pub fn download(
    rel: &Release,
    role: Role,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf> {
    let asset = rel
        .asset(role)
        .with_context(|| format!("relizda {} fayli yo'q", asset_name(role, &rel.version)))?;
    let sums = rel
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .context("relizda SHA256SUMS yo'q")?;
    let sums = String::from_utf8(fetch(&sums.url, &|_, _| {}, sums.size)?)?;
    let want = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (hash, name) = (it.next()?, it.next()?);
            (name.trim_start_matches('*') == asset.name).then(|| hash.to_lowercase())
        })
        .context("SHA256SUMS da fayl topilmadi")?;
    let data = fetch(&asset.url, progress, asset.size)?;
    let got: String = Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        bail!("fayl buzilgan (SHA-256 mos kelmadi)");
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(&asset.name);
    std::fs::write(&path, data)?;
    Ok(path)
}

/// Starts installing a downloaded console update. On Windows the installer takes over (the caller
/// should exit right away); on Ubuntu this waits for `pkexec apt-get install` (password prompt)
/// and returns once the new version is on disk.
pub fn install_console(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new(path)
            .args([
                "/SILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/TYPE=teacher",
                "/COMPONENTS=console",
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
        let tr = format!(
            "\"{file}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent"
        );
        run(
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
        )?;
        run("schtasks", &["/run", "/tn", "Vimora Update"])
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
                "install",
                "-y",
                "--reinstall",
                &file,
            ],
        )
    }
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
