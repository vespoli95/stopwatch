use std::fmt;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    assets: Vec<Asset>,
}

impl Release {
    pub fn installer_url(&self) -> Option<String> {
        self.assets
            .iter()
            .find(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name.starts_with("stopwatch-setup-") && name.ends_with(".exe")
            })
            .map(|asset| asset.browser_download_url.clone())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Unavailable,
    Checking,
    Current,
    Available,
    Installing,
    Failed(String),
}

#[derive(Debug)]
pub struct InstallError(String);

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub fn fetch_latest_release(repository: &str) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "stopwatch-updater")
        .call()
        .map_err(|error| format!("Update check failed: {error}"))?;

    let body = response
        .into_string()
        .map_err(|error| format!("Could not read the update response: {error}"))?;
    serde_json::from_str(&body).map_err(|error| format!("Invalid update response: {error}"))
}

pub fn download_and_launch_installer(url: &str) -> Result<(), InstallError> {
    if !cfg!(windows) {
        return Err(InstallError(
            "Installing updates is only supported on Windows.".to_owned(),
        ));
    }

    let destination = update_path();
    let response = ureq::get(url)
        .set("User-Agent", "stopwatch-updater")
        .call()
        .map_err(|error| InstallError(format!("Could not download the update: {error}")))?;
    let mut source = response.into_reader();
    let mut file = File::create(&destination)
        .map_err(|error| InstallError(format!("Could not save the update: {error}")))?;
    io::copy(&mut source, &mut file)
        .map_err(|error| InstallError(format!("Could not save the update: {error}")))?;
    drop(file);

    Command::new("cmd")
        .args([
            "/C",
            "start",
            "\"\"",
            destination
                .to_str()
                .expect("the system temporary path must be valid UTF-8"),
            "/SILENT",
            "/SUPPRESSMSGBOXES",
            "/CLOSEAPPLICATIONS",
            "/RESTARTAPPLICATIONS",
        ])
        .spawn()
        .map_err(|error| InstallError(format!("Could not start the installer: {error}")))?;

    Ok(())
}

fn update_path() -> PathBuf {
    std::env::temp_dir().join(format!("stopwatch-setup-{}.exe", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::Release;

    #[test]
    fn finds_windows_installer_asset() {
        let release: Release = serde_json::from_str(
            r#"{
                "tag_name": "build-12",
                "assets": [
                    {"name":"stopwatch.zip","browser_download_url":"https://example/zip"},
                    {"name":"stopwatch-setup-build-12.exe","browser_download_url":"https://example/setup"}
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(
            release.installer_url().as_deref(),
            Some("https://example/setup")
        );
    }
}
