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

## Windows download

Pushes to GitHub trigger the **Windows Release** workflow. The workflow uploads
both a downloadable artifact and a GitHub Release containing
`stopwatch-setup-<build>.exe`. Run the setup executable to install Stopwatch;
Start Menu and optional Desktop shortcuts are created.

Apps built by that workflow check for the latest release once when opened. When
an update is available, select **Install update**. An app built locally does not
check because it has no GitHub repository configured.

To build a local update-enabled binary manually:

```bash
STOPWATCH_UPDATE_REPOSITORY=owner/repository \
STOPWATCH_RELEASE_TAG=development \
cargo build --release
```

