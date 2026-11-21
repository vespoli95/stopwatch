use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

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
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build();
    let response = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "stopwatch-updater")
        .call()
        .map_err(format_fetch_error)?;

    let body = response
        .into_string()
        .map_err(|error| format!("Could not read the update response: {error}"))?;
    serde_json::from_str(&body).map_err(|error| format!("Invalid update response: {error}"))
}

fn format_fetch_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format_http_status_error(status)
            .unwrap_or_else(|| format!("Update check failed: HTTP {status}")),
        other => format!("Update check failed: {other}"),
    }
}

fn format_http_status_error(status: u16) -> Option<String> {
    (status == 404).then(|| {
        "No published release was found. Make sure the repository is public and has a release."
            .to_owned()
    })
}

pub fn download_and_launch_installer(url: &str) -> Result<(), InstallError> {
    if !cfg!(windows) {
        return Err(InstallError(
            "Installing updates is only supported on Windows.".to_owned(),
        ));
    }

    let destination = update_path();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "stopwatch-updater")
        .call()
        .map_err(|error| InstallError(format!("Could not download the update: {error}")))?;
    let mut source = response.into_reader();
    let mut file = File::create(&destination)
        .map_err(|error| InstallError(format!("Could not save the update: {error}")))?;
    if let Err(error) = io::copy(&mut source, &mut file).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&destination);
        return Err(InstallError(format!("Could not save the update: {error}")));
    }
    drop(file);

    installer_command(&destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| InstallError(format!("Could not start the installer: {error}")))?;

    Ok(())
}

fn installer_command(installer_path: &Path) -> Command {
    let mut command = Command::new(installer_path);
    command.args([
        "/SILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
        "/FORCECLOSEAPPLICATIONS",
    ]);
    command
}

fn update_path() -> PathBuf {
    std::env::temp_dir().join(format!("stopwatch-setup-{}.exe", std::process::id()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::{Release, format_http_status_error, installer_command};

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

    #[test]
    fn returns_none_when_release_has_no_windows_installer() {
        let release: Release = serde_json::from_str(
            r#"{
                "tag_name": "build-12",
                "assets": [
                    {"name":"stopwatch.exe","browser_download_url":"https://example/exe"},
                    {"name":"stopwatch-setup-build-12.zip","browser_download_url":"https://example/zip"}
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(release.installer_url(), None);
    }

    #[test]
    fn installer_command_invokes_installer_directly_with_silent_flags() {
        let installer_path =
            Path::new(r"C:\Users\Jane Doe\AppData\Local\Temp\stopwatch-setup-42.exe");
        let command = installer_command(installer_path);

        assert_eq!(command.get_program(), installer_path.as_os_str());
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(
            args,
            [
                OsStr::new("/SILENT"),
                OsStr::new("/SUPPRESSMSGBOXES"),
                OsStr::new("/NORESTART"),
                OsStr::new("/CLOSEAPPLICATIONS"),
                OsStr::new("/FORCECLOSEAPPLICATIONS"),
            ]
        );
    }

    #[test]
    fn formats_404_fetch_error_clearly() {
        assert_eq!(
            format_http_status_error(404).as_deref(),
            Some(
                "No published release was found. Make sure the repository is public and has a release."
            )
        );
        assert_eq!(format_http_status_error(500), None);
    }
}
