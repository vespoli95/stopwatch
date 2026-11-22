# Stopwatch

A small Rust desktop stopwatch with start, pause, reset, lap timing, and a
Windows Clock-style always-on-top mode. Pinning places a compact window at the
top-right of the active display and keeps it above other windows.

Absolute window placement is supported by Windows and X11. Wayland compositors
intentionally prevent applications from choosing their screen coordinates, so
the square always-on-top layout is used there but the compositor chooses where
to place it.

## Run locally

```bash
cargo run
```

## Google Sheets integration

Stopwatch can log your current elapsed time to an existing Google Sheet tab with
a single click (from either the full window or the pinned compact window)
without pausing or resetting the timer. It writes one row per day with:

- **Column A:** Date (`YYYY-MM-DD`)
- **Column B:** Minutes (rounded to the nearest whole minute)

If today's date already exists in Column A, Stopwatch overwrites Column B on
that row; otherwise it appends a new row.

### Setup

1. Open your existing Google Sheet and select **Extensions -> Apps Script**.
2. In Stopwatch, click **Sheets** (top right) and click **Copy Apps Script**,
   or copy [`google-apps-script/Code.gs`](google-apps-script/Code.gs).
3. In the Apps Script editor, select all (`Ctrl+A`), paste (`Ctrl+V`) into
   `Code.gs`, optionally set `TARGET_SHEET_NAME` to your existing tab name
   (or leave it `""` to use the first tab), and save (`Ctrl+S`).
4. *(Optional)* Set a shared secret token in `SHARED_SECRET` (or as a Script
   Property named `SHARED_SECRET`).
5. Click **Deploy -> New deployment** (or **Manage deployments -> Edit ->
   Version: New version** when updating `Code.gs`) -> **Web app**:
   - **Execute as:** Me
   - **Who has access:** Anyone
6. Paste the deployed `/exec` Web App URL (and optional secret token) into
   Stopwatch, click **Save**, and click **Test**.

Settings are stored locally in `%APPDATA%\Stopwatch\sheets.json` on Windows or
`~/.config/stopwatch/sheets.json` on Linux.

## Windows download

Pushes to GitHub trigger the **Windows Release** workflow. The workflow uploads
both a downloadable artifact and a GitHub Release containing
`stopwatch-setup-<build>.exe`. Run the setup executable to install Stopwatch;
Start Menu and optional Desktop shortcuts are created.

Apps built by that workflow check for the latest release once when opened. The
GitHub repository must be public so unauthenticated release checks and asset
downloads succeed. When an update is available, select **Install update** to
download, silently install, and relaunch Stopwatch. An app built locally does
not check because it has no GitHub repository configured.

To build a local update-enabled binary manually:

```bash
STOPWATCH_UPDATE_REPOSITORY=owner/repository \
STOPWATCH_RELEASE_TAG=local-test \
cargo build --release
```

