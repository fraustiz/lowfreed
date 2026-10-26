//! Vérification et installation des mises à jour depuis GitHub Releases.

use std::{
    fs,
    io::{Cursor, Read},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use anyhow::{anyhow, Context, Result};
use eframe::egui;
use serde::Deserialize;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "fraustiz/lowfreed";
const USER_AGENT: &str = concat!("lowfreed-updater/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn version(&self) -> &str {
        self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name)
    }

    pub fn matching_asset(&self) -> Option<&ReleaseAsset> {
        #[cfg(target_os = "windows")]
        let target = "windows";
        #[cfg(target_os = "macos")]
        let target = "macos";
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let target = "linux";

        self.assets.iter().find(|a| {
            let n = a.name.to_lowercase();
            n.contains(target) && n.ends_with(".zip")
        })
    }
}

pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.trim().strip_prefix('v').unwrap_or(v.trim());
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.split('-').next()?.parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    match (parse_version(remote), parse_version(current)) {
        (Some(r), Some(c)) => r > c,
        _ => remote != current,
    }
}

pub fn check_latest_release() -> Result<Option<(ReleaseInfo, ReleaseAsset)>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp: ReleaseInfo = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github.v3+json")
        .call()
        .context("impossible de contacter GitHub Releases")?
        .into_json()
        .context("réponse GitHub invalide")?;

    if is_newer(resp.version(), CURRENT_VERSION) {
        let asset = resp.matching_asset().cloned().ok_or_else(|| {
            anyhow!("nouvelle version {} trouvée mais aucun binaire compatible disponible", resp.tag_name)
        })?;
        Ok(Some((resp, asset)))
    } else {
        Ok(None)
    }
}

fn apply_update(archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>, current_exe: &PathBuf) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        let mut found = None;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            if name.ends_with("lowfreed.exe") && !name.ends_with("lowfreed-cli.exe") {
                let mut data = Vec::new();
                file.read_to_end(&mut data)?;
                found = Some(data);
                break;
            }
        }
        let data = found.ok_or_else(|| anyhow!("lowfreed.exe introuvable dans l'archive"))?;

        let old_exe = current_exe.with_extension("exe.old");
        let _ = fs::remove_file(&old_exe);
        fs::rename(current_exe, &old_exe)
            .context("impossible de renommer l'exécutable actuel")?;

        fs::write(current_exe, &data)
            .context("impossible d'écrire le nouvel exécutable")?;

        if let Some(dir) = current_exe.parent() {
            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    if file.name().ends_with("lowfreed-cli.exe") {
                        let cli_path = dir.join("lowfreed-cli.exe");
                        let _ = fs::remove_file(&cli_path);
                        let mut cli_data = Vec::new();
                        let _ = file.read_to_end(&mut cli_data);
                        let _ = fs::write(cli_path, cli_data);
                        break;
                    }
                }
            }
        }
        Ok(format!("Exécutable mis à jour : {}", current_exe.display()))
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut found = None;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            if (name.ends_with("/lowfreed") || name == "lowfreed") && !name.ends_with("lowfreed-cli") {
                let mut data = Vec::new();
                file.read_to_end(&mut data)?;
                found = Some(data);
                break;
            }
        }
        let data = found.ok_or_else(|| anyhow!("binaire lowfreed introuvable dans l'archive"))?;

        let temp_exe = current_exe.with_extension("new");
        fs::write(&temp_exe, &data)?;
        let mut perms = fs::metadata(&temp_exe)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_exe, perms)?;

        fs::rename(&temp_exe, current_exe)
            .context("impossible de remplacer le binaire macOS")?;

        if let Some(dir) = current_exe.parent() {
            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    let name = file.name().to_string();
                    if name.ends_with("/lowfreed-cli") || name == "lowfreed-cli" {
                        let cli_path = dir.join("lowfreed-cli");
                        let mut cli_data = Vec::new();
                        if file.read_to_end(&mut cli_data).is_ok() {
                            let temp_cli = dir.join("lowfreed-cli.new");
                            if fs::write(&temp_cli, &cli_data).is_ok() {
                                if let Ok(meta) = fs::metadata(&temp_cli) {
                                    let mut p = meta.permissions();
                                    p.set_mode(0o755);
                                    let _ = fs::set_permissions(&temp_cli, p);
                                    let _ = fs::rename(temp_cli, cli_path);
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
        Ok(format!("Binaire mis à jour : {}", current_exe.display()))
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = (archive, current_exe);
        Err(anyhow!("mise à jour automatique non prise en charge sur cette plateforme"))
    }
}

pub fn download_and_apply(
    asset_url: &str,
    on_progress: impl Fn(f32, u64, u64) + Send + Sync + 'static,
) -> Result<String> {
    let resp = ureq::get(asset_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .context("échec du téléchargement")?;

    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut bytes = Vec::with_capacity(if total > 0 { total as usize } else { 1024 * 1024 * 6 });
    let mut buf = [0u8; 16384];
    let mut downloaded = 0u64;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        downloaded += n as u64;
        let p = if total > 0 { (downloaded as f32 / total as f32).min(1.0) } else { 0.0 };
        on_progress(p, total, downloaded);
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .context("archive zip invalide ou corrompue")?;

    let current_exe = std::env::current_exe()
        .context("impossible de localiser l'exécutable actuel")?;

    apply_update(&mut archive, &current_exe)
}

pub fn restart() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    #[cfg(target_os = "macos")]
    {
        let path_str = current_exe.to_string_lossy();
        if let Some(app_idx) = path_str.find(".app/Contents/MacOS/") {
            let app_path = &path_str[..app_idx + 4];
            let _ = std::process::Command::new("open")
                .arg("-n")
                .arg(app_path)
                .spawn();
            std::process::exit(0);
        }
    }
    std::process::Command::new(current_exe).spawn()?;
    std::process::exit(0);
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: String,
        url: String,
        asset_url: String,
        asset_name: String,
        size: u64,
    },
    Downloading {
        version: String,
        progress: f32,
        downloaded: u64,
        total: u64,
    },
    Applying,
    ReadyToRestart {
        version: String,
    },
    Error(String),
}

#[derive(Clone)]
pub struct Updater {
    status: Arc<Mutex<UpdateStatus>>,
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

impl Updater {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(UpdateStatus::Idle)),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn dismiss(&self) {
        *self.status.lock().unwrap() = UpdateStatus::Idle;
    }

    pub fn check(&self, ctx: egui::Context) {
        let status = self.status.clone();
        *status.lock().unwrap() = UpdateStatus::Checking;
        ctx.request_repaint();

        thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let res = check_latest_release();
                match res {
                    Ok(Some((release, asset))) => {
                        *status.lock().unwrap() = UpdateStatus::Available {
                            version: release.tag_name.clone(),
                            notes: release.body.unwrap_or_default(),
                            url: release.html_url,
                            asset_url: asset.browser_download_url,
                            asset_name: asset.name,
                            size: asset.size,
                        };
                    }
                    Ok(None) => {
                        *status.lock().unwrap() = UpdateStatus::UpToDate;
                    }
                    Err(e) => {
                        *status.lock().unwrap() = UpdateStatus::Error(e.to_string());
                    }
                }
                ctx.request_repaint();
            })
            .expect("fil update-check");
    }

    pub fn install(&self, ctx: egui::Context) {
        let current = self.status();
        let UpdateStatus::Available { version, asset_url, .. } = current else { return };

        let status = self.status.clone();
        let ver = version.clone();
        *status.lock().unwrap() = UpdateStatus::Downloading {
            version: ver.clone(),
            progress: 0.0,
            downloaded: 0,
            total: 0,
        };
        ctx.request_repaint();

        thread::Builder::new()
            .name("update-install".into())
            .spawn(move || {
                let s = status.clone();
                let v = ver.clone();
                let c = ctx.clone();
                let on_progress = move |progress, total, downloaded| {
                    *s.lock().unwrap() = UpdateStatus::Downloading {
                        version: v.clone(),
                        progress,
                        downloaded,
                        total,
                    };
                    c.request_repaint();
                };

                let res = download_and_apply(&asset_url, on_progress);
                match res {
                    Ok(_) => {
                        *status.lock().unwrap() = UpdateStatus::ReadyToRestart { version: ver };
                    }
                    Err(e) => {
                        *status.lock().unwrap() = UpdateStatus::Error(format!("Erreur d'installation : {e:#}"));
                    }
                }
                ctx.request_repaint();
            })
            .expect("fil update-install");
    }

    pub fn restart(&self) -> Result<()> {
        restart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parsing() {
        assert_eq!(parse_version("0.3.0"), Some((0, 3, 0)));
        assert_eq!(parse_version("v0.3.1"), Some((0, 3, 1)));
        assert_eq!(parse_version("v1.12.5-rc1"), Some((1, 12, 5)));
        assert_eq!(parse_version("bad"), None);
    }

    #[test]
    fn semver_comparison() {
        assert!(is_newer("v0.3.1", "0.3.0"));
        assert!(is_newer("0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
    }

    #[test]
    fn asset_matching() {
        let info = ReleaseInfo {
            tag_name: "v0.3.0".into(),
            name: None,
            body: None,
            html_url: "https://example.com".into(),
            assets: vec![
                ReleaseAsset {
                    name: "lowfreed-v0.3.0-windows-x86_64.zip".into(),
                    browser_download_url: "https://example.com/win.zip".into(),
                    size: 1000,
                },
                ReleaseAsset {
                    name: "lowfreed-v0.3.0-macos-universal.zip".into(),
                    browser_download_url: "https://example.com/mac.zip".into(),
                    size: 2000,
                },
            ],
        };

        let asset = info.matching_asset();
        assert!(asset.is_some());
        #[cfg(target_os = "windows")]
        assert!(asset.unwrap().name.contains("windows"));
        #[cfg(target_os = "macos")]
        assert!(asset.unwrap().name.contains("macos"));
    }
}
