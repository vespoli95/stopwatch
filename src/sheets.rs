use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const APPS_SCRIPT_SOURCE: &str = include_str!("../google-apps-script/Code.gs");

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetsConfig {
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub secret: String,
}

impl SheetsConfig {
    pub fn normalized(&self) -> Self {
        Self {
            webhook_url: self.webhook_url.trim().to_owned(),
            secret: self.secret.trim().to_owned(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.webhook_url.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopwatchSnapshot {
    pub elapsed: Duration,
    pub recorded_at_unix_ms: u64,
    pub local_date: Option<String>,
}

impl StopwatchSnapshot {
    pub fn capture(elapsed: Duration) -> Self {
        Self {
            elapsed,
            recorded_at_unix_ms: current_unix_ms(),
            local_date: None,
        }
    }
}

pub fn rounded_minutes(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis().saturating_add(30_000) / 60_000).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendReceipt {
    pub spreadsheet: Option<String>,
    pub sheet: String,
    pub date: Option<String>,
    pub minutes: Option<u64>,
    pub row: Option<u64>,
    pub updated: Option<bool>,
}

#[derive(Serialize)]
struct RecordPayload<'a> {
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<&'a str>,
    elapsed_ms: u64,
    minutes: u64,
    recorded_at_unix_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<&'a str>,
}

#[derive(Serialize)]
struct PingPayload<'a> {
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<&'a str>,
    recorded_at_unix_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct WebhookResponse {
    ok: bool,
    spreadsheet: Option<String>,
    sheet: Option<String>,
    date: Option<String>,
    minutes: Option<u64>,
    row: Option<u64>,
    updated: Option<bool>,
    error: Option<String>,
}

pub fn validate_webhook_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("Enter your Google Apps Script Web App URL.".to_owned());
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("docs.google.com/spreadsheets") {
        return Err(
            "That looks like a Google Sheets URL. Paste the deployed Apps Script Web App URL (https://script.google.com/macros/s/.../exec) instead."
                .to_owned(),
        );
    }

    if !trimmed.starts_with("https://") {
        return Err("Webhook URL must start with https://.".to_owned());
    }

    if trimmed.chars().any(char::is_whitespace) {
        return Err("Webhook URL cannot contain spaces.".to_owned());
    }

    let without_query = trimmed
        .split(['?', '#'])
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    let lower_path = without_query.to_ascii_lowercase();
    if lower.contains("script.google.com") {
        if lower_path.contains("/home/projects/") || lower_path.ends_with("/edit") {
            return Err(
                "That looks like the Apps Script editor URL, not the deployed Web App URL. Click Deploy -> New deployment -> Web app and copy the URL ending in /exec."
                    .to_owned(),
            );
        }
        if lower_path.ends_with("/dev") {
            return Err(
                "Use the deployed /exec Web App URL instead of the /dev test URL.".to_owned(),
            );
        }
        if !lower_path.ends_with("/exec") {
            return Err(
                "Google Apps Script Web App URLs must end in /exec (copy it from Deploy -> Manage deployments -> Web app URL)."
                    .to_owned(),
            );
        }
    }

    Ok(trimmed.to_owned())
}

pub fn config_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|val| !val.is_empty()) {
            return Ok(PathBuf::from(appdata)
                .join("Stopwatch")
                .join("sheets.json"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE").filter(|val| !val.is_empty()) {
            return Ok(PathBuf::from(profile)
                .join(".config")
                .join("stopwatch")
                .join("sheets.json"));
        }
        return Err("Could not determine the configuration directory.".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .filter(|val| !val.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Stopwatch")
                    .join("sheets.json")
            })
            .ok_or_else(|| "Could not determine the configuration directory.".to_owned());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|val| !val.is_empty()) {
            return Ok(PathBuf::from(xdg).join("stopwatch").join("sheets.json"));
        }
        return std::env::var_os("HOME")
            .filter(|val| !val.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join(".config")
                    .join("stopwatch")
                    .join("sheets.json")
            })
            .ok_or_else(|| "Could not determine the configuration directory.".to_owned());
    }

    #[cfg(not(any(target_os = "windows", unix)))]
    {
        Err("Could not determine the configuration directory.".to_owned())
    }
}

pub fn load_config() -> Result<SheetsConfig, String> {
    let path = config_path()?;
    load_config_from(&path)
}

pub fn save_config(config: &SheetsConfig) -> Result<(), String> {
    let path = config_path()?;
    save_config_to(&path, config)
}

fn load_config_from(path: &Path) -> Result<SheetsConfig, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SheetsConfig::default());
        }
        Err(error) => return Err(format!("Could not read Google Sheets settings: {error}")),
    };

    serde_json::from_str::<SheetsConfig>(&contents)
        .map(|config| config.normalized())
        .map_err(|error| format!("Invalid Google Sheets settings file: {error}"))
}

fn save_config_to(path: &Path, config: &SheetsConfig) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create settings directory: {error}"))?;
    }

    let normalized = config.normalized();
    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|error| format!("Could not serialize Google Sheets settings: {error}"))?;

    let tmp_path = path.with_extension("json.tmp");
    if let Err(error) = fs::write(&tmp_path, format!("{json}\n")) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Could not save Google Sheets settings: {error}"));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600)) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!(
                "Could not set permissions on Google Sheets settings: {error}"
            ));
        }
    }

    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Could not save Google Sheets settings: {error}"));
    }

    Ok(())
}

pub fn send_snapshot(
    config: &SheetsConfig,
    mut snapshot: StopwatchSnapshot,
) -> Result<SendReceipt, String> {
    let normalized = config.normalized();
    let url = validate_webhook_url(&normalized.webhook_url)?;
    if snapshot.local_date.is_none() {
        snapshot.local_date = current_local_date();
    }
    send_snapshot_to_url(&url, &normalized.secret, snapshot)
}

pub fn test_connection(config: &SheetsConfig) -> Result<SendReceipt, String> {
    let normalized = config.normalized();
    let url = validate_webhook_url(&normalized.webhook_url)?;
    test_connection_to_url(&url, &normalized.secret, current_unix_ms(), current_local_date())
}

fn send_snapshot_to_url(
    url: &str,
    secret: &str,
    snapshot: StopwatchSnapshot,
) -> Result<SendReceipt, String> {
    let minutes = rounded_minutes(snapshot.elapsed);
    let payload = build_record_payload(secret, &snapshot, minutes);
    let body = serde_json::to_string(&payload)
        .map_err(|error| format!("Could not prepare Google Sheets payload: {error}"))?;
    let response = post_json(url, &body)?;
    Ok(SendReceipt {
        spreadsheet: response.spreadsheet,
        sheet: response.sheet.unwrap_or_else(|| "Sheet".to_owned()),
        date: response.date.or(snapshot.local_date),
        minutes: response.minutes.or(Some(minutes)),
        row: response.row,
        updated: response.updated,
    })
}

fn test_connection_to_url(
    url: &str,
    secret: &str,
    recorded_at_unix_ms: u64,
    local_date: Option<String>,
) -> Result<SendReceipt, String> {
    let payload = build_ping_payload(secret, recorded_at_unix_ms, local_date.as_deref());
    let body = serde_json::to_string(&payload)
        .map_err(|error| format!("Could not prepare Google Sheets ping payload: {error}"))?;
    let response = post_json(url, &body)?;
    Ok(SendReceipt {
        spreadsheet: response.spreadsheet,
        sheet: response.sheet.unwrap_or_else(|| "Sheet".to_owned()),
        date: response.date.or(local_date),
        minutes: None,
        row: response.row,
        updated: None,
    })
}

fn build_record_payload<'a>(
    secret: &'a str,
    snapshot: &'a StopwatchSnapshot,
    minutes: u64,
) -> RecordPayload<'a> {
    let trimmed_secret = secret.trim();
    RecordPayload {
        action: "record",
        secret: (!trimmed_secret.is_empty()).then_some(trimmed_secret),
        elapsed_ms: u64::try_from(snapshot.elapsed.as_millis()).unwrap_or(u64::MAX),
        minutes,
        recorded_at_unix_ms: snapshot.recorded_at_unix_ms,
        date: snapshot.local_date.as_deref(),
    }
}

fn build_ping_payload<'a>(
    secret: &'a str,
    recorded_at_unix_ms: u64,
    local_date: Option<&'a str>,
) -> PingPayload<'a> {
    let trimmed_secret = secret.trim();
    PingPayload {
        action: "ping",
        secret: (!trimmed_secret.is_empty()).then_some(trimmed_secret),
        recorded_at_unix_ms,
        date: local_date,
    }
}

const MAX_REDIRECTS: usize = 8;

fn post_json(url: &str, body: &str) -> Result<WebhookResponse, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build();

    let mut current_url = url.to_owned();
    let mut send_post = true;
    let mut hops = 0usize;

    let response = loop {
        let resp = if send_post {
            agent
                .post(&current_url)
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .set("User-Agent", "stopwatch-sheets")
                .send_string(body)
        } else {
            agent
                .get(&current_url)
                .set("Accept", "application/json")
                .set("User-Agent", "stopwatch-sheets")
                .call()
        }
        .map_err(format_request_error)?;

        let status = resp.status();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let Some(location) = resp.header("location") else {
                break resp;
            };
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err("Google Sheets request exceeded maximum redirects.".to_owned());
            }

            let next_url = resolve_redirect_url(&current_url, location)?;
            let from_https = current_url
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
            let to_http = next_url
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"));
            if from_https && to_http {
                return Err("Refused insecure redirect from HTTPS to HTTP.".to_owned());
            }
            if is_google_signin_url(&next_url) {
                return Err(signin_redirect_error());
            }
            if matches!(status, 301 | 302 | 303) {
                send_post = false;
            }
            current_url = next_url;
            continue;
        }

        break resp;
    };

    let status = response.status();
    let final_url = response.get_url().to_owned();
    let response_body = response
        .into_string()
        .map_err(|error| format!("Could not read Google Sheets response: {error}"))?;
    let trimmed_body = response_body.trim();

    if is_google_signin_url(&final_url) {
        return Err(signin_redirect_error());
    }

    if trimmed_body.starts_with('<') {
        return Err(diagnose_html_response(status, &final_url, trimmed_body));
    }

    let parsed: WebhookResponse = serde_json::from_str(trimmed_body)
        .map_err(|error| format!("Unexpected response from Google Sheets script: {error}"))?;

    if !parsed.ok {
        return Err(parsed
            .error
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or_else(|| "Google Sheets script returned an error.".to_owned()));
    }

    Ok(parsed)
}

fn resolve_redirect_url(base: &str, location: &str) -> Result<String, String> {
    let trimmed = location.trim();
    if trimmed.is_empty() {
        return Err("Google Sheets returned an empty redirect location.".to_owned());
    }

    if trimmed
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || trimmed
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        return Ok(trimmed.to_owned());
    }

    if trimmed.starts_with("//") {
        let scheme = base
            .split_once("://")
            .map_or("https", |(scheme, _)| scheme);
        return Ok(format!("{scheme}:{trimmed}"));
    }

    let Some(scheme_end) = base.find("://").map(|idx| idx + 3) else {
        return Err(format!("Invalid redirect base URL: {base}"));
    };

    if trimmed.starts_with('/') {
        let authority_end = base[scheme_end..]
            .find(['/', '?', '#'])
            .map_or(base.len(), |rel| scheme_end + rel);
        return Ok(format!("{}{trimmed}", &base[..authority_end]));
    }

    let base_no_fragment = base.split('#').next().unwrap_or(base);
    let base_path = base_no_fragment.split('?').next().unwrap_or(base_no_fragment);
    if trimmed.starts_with('?') {
        return Ok(format!("{base_path}{trimmed}"));
    }

    if let Some(last_slash_rel) = base_path[scheme_end..].rfind('/') {
        let dir = &base_path[..scheme_end + last_slash_rel + 1];
        Ok(format!("{dir}{trimmed}"))
    } else {
        Ok(format!("{base_path}/{trimmed}"))
    }
}

fn extract_host(url: &str) -> &str {
    let without_scheme = url.find("://").map_or(url, |idx| &url[idx + 3..]);
    let host_port = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host_port
        .rsplit_once('@')
        .map_or(host_port, |(_, after)| after);
    host.split(':').next().unwrap_or(host)
}

fn host_is_google(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    lower == "google.com" || lower.ends_with(".google.com")
}

fn is_google_signin_url(url: &str) -> bool {
    let host = extract_host(url);
    host.eq_ignore_ascii_case("accounts.google.com") || url.contains("ServiceLogin")
}

fn signin_redirect_error() -> String {
    "Google redirected to a sign-in page. In Apps Script, open Deploy -> Manage deployments -> Edit -> set 'Who has access' to 'Anyone' (not 'Only myself' or 'Anyone with Google account') and click Deploy. Note: some Google Workspace (work/school) accounts block 'Anyone' Web Apps."
        .to_owned()
}

fn diagnose_html_response(status: u16, final_url: &str, html: &str) -> String {
    let title = extract_html_title(html);
    let text = strip_html_to_text(html);
    let lower_text = text.to_ascii_lowercase();
    let host = extract_host(final_url);

    if lower_text.contains("script function not found: dopost") {
        return "Apps Script error: 'Script function not found: doPost'. Save Code.gs first, then deploy a New version (Deploy -> Manage deployments -> Edit -> Version: New version -> Deploy)."
            .to_owned();
    }
    if lower_text.contains("authorization is required") {
        return "Apps Script error: Authorization required. Redeploy the Web App with 'Execute as: Me' and approve the Google permissions prompt."
            .to_owned();
    }
    if lower_text.contains("sorry, unable to open the file at this time")
        || lower_text.contains("page not found")
    {
        return "Google could not open that Web App URL. Verify you copied the exact /exec URL from Deploy -> Manage deployments."
            .to_owned();
    }
    if (lower_text.contains("sign in") || lower_text.contains("signin")) && host_is_google(host) {
        return signin_redirect_error();
    }

    let snippet = truncate_chars(&text, 160);
    let detail = match (title, snippet.is_empty()) {
        (Some(title), false) if !snippet.eq_ignore_ascii_case(&title) => {
            format!("{title} - {snippet}")
        }
        (Some(title), _) => title,
        (None, false) => snippet,
        (None, true) => String::new(),
    };

    if detail.is_empty() {
        format!("Google returned an HTML page (HTTP {status} from {host}).")
    } else {
        format!("Google returned an HTML page (HTTP {status} from {host}): {detail}")
    }
}

fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start_tag = lower.find("<title")?;
    let content_rel = lower[start_tag..].find('>')?;
    let content_start = start_tag + content_rel + 1;
    let end_rel = lower[content_start..].find("</title>")?;
    let raw_title = &html[content_start..content_start + end_rel];
    let decoded = decode_and_collapse_html_text(raw_title);
    (!decoded.is_empty()).then_some(decoded)
}

fn strip_html_to_text(html: &str) -> String {
    let without_blocks = strip_tag_blocks(&strip_tag_blocks(html, "style"), "script");
    let mut visible = String::with_capacity(without_blocks.len());
    let mut in_tag = false;
    for ch in without_blocks.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !visible.ends_with(' ') {
                    visible.push(' ');
                }
            }
            '>' => {
                in_tag = false;
            }
            _ if !in_tag => visible.push(ch),
            _ => {}
        }
    }
    decode_and_collapse_html_text(&visible)
}

fn strip_tag_blocks(html: &str, tag: &str) -> String {
    let open_pattern = format!("<{tag}");
    let close_pattern = format!("</{tag}>");
    let lower = html.to_ascii_lowercase();
    let mut result = String::with_capacity(html.len());
    let mut cursor = 0usize;

    while let Some(rel_start) = lower[cursor..].find(&open_pattern) {
        let start = cursor + rel_start;
        let after_open = start + open_pattern.len();
        let next_char = lower.as_bytes().get(after_open).copied();
        if !matches!(next_char, Some(b'>' | b' ' | b'\t' | b'\r' | b'\n' | b'/')) {
            result.push_str(&html[cursor..after_open]);
            cursor = after_open;
            continue;
        }
        result.push_str(&html[cursor..start]);
        if let Some(rel_end) = lower[after_open..].find(&close_pattern) {
            cursor = after_open + rel_end + close_pattern.len();
            result.push(' ');
        } else {
            cursor = html.len();
            break;
        }
    }
    result.push_str(&html[cursor..]);
    result
}

fn decode_and_collapse_html_text(raw: &str) -> String {
    let decoded = raw
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    let mut out = String::with_capacity(decoded.len());
    for word in decoded.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn format_request_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(401 | 403, _) => {
            "Google Sheets denied access (HTTP 401/403). Deploy the Apps Script Web App with 'Who has access: Anyone'."
                .to_owned()
        }
        ureq::Error::Status(404, _) => {
            "Web App URL not found (HTTP 404). Verify the deployed /exec URL.".to_owned()
        }
        ureq::Error::Status(status, _) => format!("Google Sheets request failed (HTTP {status})."),
        other => format!("Could not reach Google Sheets: {other}"),
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct SystemTimeWin {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(system_time: *mut SystemTimeWin);
}

#[cfg(target_os = "windows")]
fn current_local_date() -> Option<String> {
    let mut local = SystemTimeWin::default();
    // SAFETY: `local` is an initialized stack struct whose pointer is valid for GetLocalTime.
    unsafe {
        GetLocalTime(&mut local);
    }
    Some(format!("{:04}-{:02}-{:02}", local.year, local.month, local.day))
}

#[cfg(unix)]
fn current_local_date() -> Option<String> {
    let output = std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_date_command_output(&text)
}

#[cfg(not(any(target_os = "windows", unix)))]
fn current_local_date() -> Option<String> {
    None
}

fn parse_date_command_output(output: &str) -> Option<String> {
    let trimmed = output.trim();
    is_valid_yyyy_mm_dd(trimmed).then(|| trimmed.to_owned())
}

fn is_valid_yyyy_mm_dd(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 4 || idx == 7 || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::{
        SheetsConfig, StopwatchSnapshot, build_ping_payload, build_record_payload,
        diagnose_html_response, load_config_from, parse_date_command_output, rounded_minutes,
        save_config_to, send_snapshot_to_url, test_connection_to_url, validate_webhook_url,
    };

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let path = std::env::temp_dir().join(format!(
                "stopwatch-sheets-test-{name}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[derive(Debug)]
    struct RecordedHttpRequest {
        method: String,
        target: String,
        content_type: Option<String>,
        body: String,
    }

    fn read_http_request(stream: &mut TcpStream) -> RecordedHttpRequest {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_owned();
        let target = parts.next().unwrap_or("").to_owned();

        let mut content_length = 0usize;
        let mut content_type = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                let value = value.trim();
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.parse().unwrap_or(0);
                } else if name.eq_ignore_ascii_case("content-type") {
                    content_type = Some(value.to_owned());
                }
            }
        }

        let mut body_bytes = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body_bytes).unwrap();
        }
        let body = String::from_utf8(body_bytes).unwrap();

        RecordedHttpRequest {
            method,
            target,
            content_type,
            body,
        }
    }

    #[test]
    fn validates_webhook_urls() {
        let consumer = "  https://script.google.com/macros/s/abc123/exec  ";
        assert_eq!(
            validate_webhook_url(consumer).unwrap(),
            "https://script.google.com/macros/s/abc123/exec"
        );
        assert_eq!(
            validate_webhook_url("https://script.google.com/macros/s/abc123/exec/").unwrap(),
            "https://script.google.com/macros/s/abc123/exec/"
        );

        let workspace = "https://script.google.com/a/macros/example.com/s/abc123/exec";
        assert_eq!(validate_webhook_url(workspace).unwrap(), workspace);

        assert!(
            validate_webhook_url("")
                .unwrap_err()
                .contains("Web App URL")
        );
        assert!(
            validate_webhook_url("http://script.google.com/macros/s/abc/exec")
                .unwrap_err()
                .contains("https://")
        );
        assert!(
            validate_webhook_url("https://docs.google.com/spreadsheets/d/12345/edit")
                .unwrap_err()
                .contains("Google Sheets URL")
        );
        assert!(
            validate_webhook_url("https://script.google.com/home/projects/abc123/edit")
                .unwrap_err()
                .contains("editor URL")
        );
        assert!(
            validate_webhook_url("https://script.google.com/macros/s/abc")
                .unwrap_err()
                .contains("must end in /exec")
        );
        assert!(
            validate_webhook_url("https://script.google.com/macros/s/abc/dev")
                .unwrap_err()
                .contains("/exec")
        );
        assert!(
            validate_webhook_url("https://script.google.com/macros/s/ab c/exec")
                .unwrap_err()
                .contains("spaces")
        );
    }

    #[test]
    fn config_round_trip_and_error_handling() {
        let temp = TempDir::new("config");
        let path = temp.path.join("nested").join("sheets.json");

        assert_eq!(load_config_from(&path).unwrap(), SheetsConfig::default());

        let config = SheetsConfig {
            webhook_url: "  https://script.google.com/macros/s/abc/exec  ".to_owned(),
            secret: "  my-secret  ".to_owned(),
        };
        save_config_to(&path, &config).unwrap();

        let loaded = load_config_from(&path).unwrap();
        assert_eq!(
            loaded,
            SheetsConfig {
                webhook_url: "https://script.google.com/macros/s/abc/exec".to_owned(),
                secret: "my-secret".to_owned(),
            }
        );

        fs::write(&path, "{not valid json").unwrap();
        assert!(
            load_config_from(&path)
                .unwrap_err()
                .contains("Invalid Google Sheets settings file")
        );
    }

    #[test]
    fn rounds_elapsed_duration_to_nearest_whole_minute() {
        assert_eq!(rounded_minutes(Duration::ZERO), 0);
        assert_eq!(rounded_minutes(Duration::from_millis(29_999)), 0);
        assert_eq!(rounded_minutes(Duration::from_millis(30_000)), 1);
        assert_eq!(rounded_minutes(Duration::from_millis(89_999)), 1);
        assert_eq!(rounded_minutes(Duration::from_millis(90_000)), 2);
        assert_eq!(rounded_minutes(Duration::from_millis(780_000)), 13);
    }

    #[test]
    fn serializes_record_and_ping_payloads_and_parses_date_output() {
        let snapshot = StopwatchSnapshot {
            elapsed: Duration::from_millis(90_250),
            recorded_at_unix_ms: 1_700_000_000_123,
            local_date: Some("2026-11-22".to_owned()),
        };

        let record_without_secret =
            serde_json::to_value(build_record_payload("   ", &snapshot, 2)).unwrap();
        assert_eq!(record_without_secret["action"], "record");
        assert!(record_without_secret.get("secret").is_none());
        assert_eq!(record_without_secret["elapsed_ms"], 90_250);
        assert_eq!(record_without_secret["minutes"], 2);
        assert_eq!(record_without_secret["date"], "2026-11-22");
        assert!(record_without_secret.get("note").is_none());
        assert!(record_without_secret.get("laps").is_none());

        let ping_with_secret = serde_json::to_value(build_ping_payload(
            "top-secret",
            1_700_000_000_999,
            Some("2026-11-22"),
        ))
        .unwrap();
        assert_eq!(ping_with_secret["action"], "ping");
        assert_eq!(ping_with_secret["secret"], "top-secret");
        assert_eq!(ping_with_secret["date"], "2026-11-22");

        assert_eq!(
            parse_date_command_output("2026-11-22\n"),
            Some("2026-11-22".to_owned())
        );
        assert_eq!(parse_date_command_output("bad-output"), None);
    }

    #[test]
    fn send_snapshot_follows_apps_script_302_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut first_stream, _) = listener.accept().unwrap();
            let first_req = read_http_request(&mut first_stream);
            first_stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: /macros/echo?lib=123\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            drop(first_stream);

            let (mut second_stream, _) = listener.accept().unwrap();
            let second_req = read_http_request(&mut second_stream);
            let body = r#"{"ok":true,"spreadsheet":"Time Log","sheet":"Sheet1","date":"2026-11-22","minutes":2,"row":4,"updated":true}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            second_stream.write_all(reply.as_bytes()).unwrap();
            (first_req, second_req)
        });

        let snapshot = StopwatchSnapshot {
            elapsed: Duration::from_millis(90_250),
            recorded_at_unix_ms: 1_700_000_000_000,
            local_date: Some("2026-11-22".to_owned()),
        };

        let url = format!("http://{addr}/macros/s/abc/exec");
        let receipt = send_snapshot_to_url(&url, "secret-1", snapshot).unwrap();
        let (first_req, second_req) = server.join().unwrap();

        assert_eq!(first_req.method, "POST");
        assert_eq!(first_req.target, "/macros/s/abc/exec");
        assert_eq!(first_req.content_type.as_deref(), Some("application/json"));
        let sent_json: Value = serde_json::from_str(&first_req.body).unwrap();
        assert_eq!(sent_json["action"], "record");
        assert_eq!(sent_json["secret"], "secret-1");
        assert_eq!(sent_json["elapsed_ms"], 90_250);
        assert_eq!(sent_json["minutes"], 2);
        assert_eq!(sent_json["date"], "2026-11-22");

        assert_eq!(second_req.method, "GET");
        assert_eq!(second_req.target, "/macros/echo?lib=123");
        assert_eq!(second_req.content_type, None);

        assert_eq!(receipt.spreadsheet.as_deref(), Some("Time Log"));
        assert_eq!(receipt.sheet, "Sheet1");
        assert_eq!(receipt.date.as_deref(), Some("2026-11-22"));
        assert_eq!(receipt.minutes, Some(2));
        assert_eq!(receipt.row, Some(4));
        assert_eq!(receipt.updated, Some(true));
    }

    #[test]
    fn send_snapshot_preserves_post_on_307_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut first_stream, _) = listener.accept().unwrap();
            let first_req = read_http_request(&mut first_stream);
            first_stream
                .write_all(
                    b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /v2/exec\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            drop(first_stream);

            let (mut second_stream, _) = listener.accept().unwrap();
            let second_req = read_http_request(&mut second_stream);
            let body = r#"{"ok":true,"spreadsheet":"Time Log","sheet":"Sheet1","date":"2026-11-22","minutes":1,"row":7,"updated":false}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            second_stream.write_all(reply.as_bytes()).unwrap();
            (first_req, second_req)
        });

        let snapshot = StopwatchSnapshot {
            elapsed: Duration::from_secs(60),
            recorded_at_unix_ms: 1_700_000_000_000,
            local_date: Some("2026-11-22".to_owned()),
        };

        let url = format!("http://{addr}/exec");
        let receipt = send_snapshot_to_url(&url, "secret-307", snapshot).unwrap();
        let (first_req, second_req) = server.join().unwrap();

        assert_eq!(first_req.method, "POST");
        assert_eq!(second_req.method, "POST");
        assert_eq!(second_req.target, "/v2/exec");
        assert_eq!(second_req.content_type.as_deref(), Some("application/json"));
        let resent_json: Value = serde_json::from_str(&second_req.body).unwrap();
        assert_eq!(resent_json["secret"], "secret-307");
        assert_eq!(resent_json["minutes"], 1);
        assert_eq!(receipt.row, Some(7));
        assert_eq!(receipt.updated, Some(false));
    }

    #[test]
    fn surfaces_script_json_errors_signin_redirects_and_html_diagnostics() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            for idx in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = read_http_request(&mut stream);
                let reply = match idx {
                    0 => {
                        let body = r#"{"ok":false,"error":"Invalid secret token."}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    1 => "HTTP/1.1 302 Found\r\nLocation: https://accounts.google.com/ServiceLogin?continue=https://script.google.com\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                    2 => {
                        let body = "<!doctype html><html><head><title>Error</title></head><body>Script function not found: doPost</body></html>";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    _ => {
                        let body = "<!doctype html><html><head><title>Error</title><style>body{color:red}</style><script>var x=1;</script></head><body><p>Custom Google error</p></body></html>";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                };
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });

        let url = format!("http://{addr}/exec");
        let snapshot = StopwatchSnapshot {
            elapsed: Duration::from_secs(10),
            recorded_at_unix_ms: 1_700_000_000_000,
            local_date: Some("2026-11-22".to_owned()),
        };

        let json_err = send_snapshot_to_url(&url, "", snapshot.clone()).unwrap_err();
        assert_eq!(json_err, "Invalid secret token.");

        let signin_err = send_snapshot_to_url(&url, "", snapshot.clone()).unwrap_err();
        assert!(signin_err.contains("Who has access"));
        assert!(signin_err.contains("Anyone"));

        let missing_dopost_err = send_snapshot_to_url(&url, "", snapshot.clone()).unwrap_err();
        assert!(missing_dopost_err.contains("Script function not found: doPost"));
        assert!(missing_dopost_err.contains("New version"));

        let generic_html_err = send_snapshot_to_url(&url, "", snapshot).unwrap_err();
        assert!(generic_html_err.contains("HTTP 200"));
        assert!(generic_html_err.contains("127.0.0.1"));
        assert!(generic_html_err.contains("Error"));
        assert!(generic_html_err.contains("Custom Google error"));
        assert!(!generic_html_err.contains("color:red"));
        assert!(!generic_html_err.contains("var x=1"));

        server.join().unwrap();

        let auth_err = diagnose_html_response(
            200,
            "https://script.google.com/macros/s/abc/exec",
            "<html><body>Authorization is required to perform that action.</body></html>",
        );
        assert!(auth_err.contains("Authorization required"));
    }

    #[test]
    fn test_connection_sends_ping_and_parses_receipt() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let req = read_http_request(&mut stream);
            let body = r#"{"ok":true,"action":"ping","spreadsheet":"Work Hours","sheet":"Daily Log","date":"2026-11-22"}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(reply.as_bytes()).unwrap();
            req
        });

        let url = format!("http://{addr}/exec");
        let receipt = test_connection_to_url(
            &url,
            "ping-secret",
            1_700_000_000_000,
            Some("2026-11-22".to_owned()),
        )
        .unwrap();
        let req = server.join().unwrap();

        let ping_json: Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(ping_json["action"], "ping");
        assert_eq!(ping_json["secret"], "ping-secret");
        assert_eq!(receipt.spreadsheet.as_deref(), Some("Work Hours"));
        assert_eq!(receipt.sheet, "Daily Log");
        assert_eq!(receipt.date.as_deref(), Some("2026-11-22"));
        assert_eq!(receipt.minutes, None);
        assert_eq!(receipt.updated, None);
    }
}
