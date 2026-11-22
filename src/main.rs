#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native_windows;
mod sheets;
mod stopwatch;
mod updater;

use std::sync::mpsc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2,
    ViewportCommand, WindowLevel,
};

use sheets::{APPS_SCRIPT_SOURCE, SendReceipt, SheetsConfig, StopwatchSnapshot};
use stopwatch::Stopwatch;
use updater::{InstallError, Release, UpdateState};

const APP_NAME: &str = "Stopwatch";
const CURRENT_RELEASE: &str = match option_env!("STOPWATCH_RELEASE_TAG") {
    Some(tag) => tag,
    None => "development",
};
const UPDATE_REPOSITORY: Option<&str> = option_env!("STOPWATCH_UPDATE_REPOSITORY");
const BACKGROUND: Color32 = Color32::from_rgb(21, 32, 52);
const PANEL: Color32 = Color32::from_rgb(31, 47, 72);
const PANEL_LIGHT: Color32 = Color32::from_rgb(43, 62, 91);
const ACCENT: Color32 = Color32::from_rgb(145, 184, 220);
const TEXT: Color32 = Color32::from_rgb(235, 241, 249);
const MUTED_TEXT: Color32 = Color32::from_rgb(215, 225, 238);
const SUCCESS_TEXT: Color32 = Color32::from_rgb(134, 214, 165);
const ERROR_TEXT: Color32 = Color32::from_rgb(244, 143, 143);
const COMPACT_SIZE: f32 = 200.0;
const UNPINNED_SIZE: [f32; 2] = [560.0, 520.0];
const TIMER_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const PINNED_TIMER_REFRESH_INTERVAL: Duration = Duration::from_millis(125);
const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_millis(500);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size(UNPINNED_SIZE)
            .with_min_inner_size([400.0, 300.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|creation_context| {
            StopwatchApp::apply_theme(&creation_context.egui_ctx);
            Ok(Box::new(StopwatchApp::new()))
        }),
    )
}

enum SheetsActionResult {
    Recorded(SendReceipt),
    Tested(SendReceipt),
}

#[derive(Default)]
enum SheetsStatus {
    #[default]
    Idle,
    Sending(u64),
    Testing,
    Sent(SendReceipt),
    Connected(SendReceipt),
    Failed(String),
}

struct StopwatchApp {
    stopwatch: Stopwatch,
    pinned: bool,
    release: Option<Release>,
    update_state: UpdateState,
    update_rx: Option<mpsc::Receiver<Result<Release, String>>>,
    install_rx: Option<mpsc::Receiver<Result<(), InstallError>>>,
    launch_update_checked: bool,
    show_laps: bool,
    pending_pin_position_frames: u8,
    sheets_config: SheetsConfig,
    sheets_url_draft: String,
    sheets_secret_draft: String,
    show_sheets_settings: bool,
    show_sheets_secret: bool,
    sheets_status: SheetsStatus,
    sheets_rx: Option<mpsc::Receiver<Result<SheetsActionResult, String>>>,
    sheets_feedback: Option<String>,
}

impl Default for StopwatchApp {
    fn default() -> Self {
        Self {
            stopwatch: Stopwatch::new(),
            pinned: false,
            release: None,
            update_state: UpdateState::Idle,
            update_rx: None,
            install_rx: None,
            launch_update_checked: false,
            show_laps: true,
            pending_pin_position_frames: 0,
            sheets_config: SheetsConfig::default(),
            sheets_url_draft: String::new(),
            sheets_secret_draft: String::new(),
            show_sheets_settings: false,
            show_sheets_secret: false,
            sheets_status: SheetsStatus::Idle,
            sheets_rx: None,
            sheets_feedback: None,
        }
    }
}

impl StopwatchApp {
    fn new() -> Self {
        let mut app = Self::default();
        match sheets::load_config() {
            Ok(config) => {
                app.sheets_url_draft = config.webhook_url.clone();
                app.sheets_secret_draft = config.secret.clone();
                app.sheets_config = config;
            }
            Err(error) => {
                app.sheets_status = SheetsStatus::Failed(error);
            }
        }
        app
    }
    fn apply_theme(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(TEXT);
        visuals.window_fill = BACKGROUND;
        visuals.panel_fill = BACKGROUND;
        visuals.extreme_bg_color = PANEL_LIGHT;
        visuals.faint_bg_color = PANEL_LIGHT;
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = Stroke::new(1.5_f32, TEXT);
        visuals.widgets.noninteractive.bg_fill = PANEL;
        visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
        visuals.widgets.inactive.weak_bg_fill = PANEL;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(57, 82, 116);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(57, 82, 116);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.active.bg_fill = Color32::from_rgb(112, 151, 190);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.open.bg_fill = PANEL_LIGHT;
        ctx.set_visuals(visuals);
    }

    fn toggle_pinned(&mut self, ctx: &egui::Context) {
        self.pinned = !self.pinned;
        let level = if self.pinned {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        ctx.send_viewport_cmd(ViewportCommand::Decorations(!self.pinned));
        let (width, height) = if self.pinned {
            (COMPACT_SIZE, COMPACT_SIZE)
        } else {
            (UNPINNED_SIZE[0], UNPINNED_SIZE[1])
        };
        ctx.send_viewport_cmd(ViewportCommand::Resizable(!self.pinned));
        if self.pinned {
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(width, height)));
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(Vec2::new(width, height)));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(400.0, 300.0)));
            let monitor_size = ctx
                .input(|input| input.viewport().monitor_size)
                .unwrap_or(Vec2::new(4096.0, 2160.0));
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(monitor_size));
        }
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(width, height)));

        if self.pinned {
            self.pending_pin_position_frames = 3;
        }
    }

    fn position_pinned_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.pending_pin_position_frames == 0 || !self.pinned {
            return;
        }

        let (monitor_size, outer_width) = ctx.input(|input| {
            (
                input.viewport().monitor_size,
                input
                    .viewport()
                    .outer_rect
                    .map_or(COMPACT_SIZE, |rect| rect.width()),
            )
        });
        if let Some(monitor_size) = monitor_size {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
                (monitor_size.x - outer_width - 16.0).max(0.0),
                16.0,
            )));
        }
        if self.pinned {
            native_windows::position_top_right(frame);
        }
        self.pending_pin_position_frames -= 1;
        ctx.request_repaint();
    }

    fn check_for_updates(&mut self, ctx: &egui::Context) {
        if self.update_rx.is_some() {
            return;
        }

        if CURRENT_RELEASE == "development" {
            self.update_state = UpdateState::Unavailable;
            return;
        }

        let Some(repository) = UPDATE_REPOSITORY else {
            self.update_state = UpdateState::Unavailable;
            return;
        };

        self.update_state = UpdateState::Checking;
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        let repository = repository.to_owned();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(updater::fetch_latest_release(&repository));
            repaint_ctx.request_repaint();
        });
        ctx.request_repaint();
    }

    fn handle_background_work(&mut self, ctx: &egui::Context) {
        match self.update_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(result)) => {
                self.update_rx = None;
                match result {
                    Ok(release) if release.tag_name != CURRENT_RELEASE => {
                        self.release = Some(release);
                        self.update_state = UpdateState::Available;
                    }
                    Ok(_) => {
                        self.release = None;
                        self.update_state = UpdateState::Current;
                    }
                    Err(message) => self.update_state = UpdateState::Failed(message),
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.update_rx = None;
                self.update_state =
                    UpdateState::Failed("Update check stopped unexpectedly.".to_owned());
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        match self.install_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(Ok(()))) => {
                self.install_rx = None;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Some(Ok(Err(error))) => {
                self.install_rx = None;
                self.update_state = UpdateState::Failed(error.to_string());
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.install_rx = None;
                self.update_state =
                    UpdateState::Failed("Update install stopped unexpectedly.".to_owned());
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        match self.sheets_rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(Ok(SheetsActionResult::Recorded(receipt)))) => {
                self.sheets_rx = None;
                self.sheets_status = SheetsStatus::Sent(receipt);
            }
            Some(Ok(Ok(SheetsActionResult::Tested(receipt)))) => {
                self.sheets_rx = None;
                self.sheets_status = SheetsStatus::Connected(receipt);
            }
            Some(Ok(Err(message))) => {
                self.sheets_rx = None;
                self.sheets_status = SheetsStatus::Failed(message);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.sheets_rx = None;
                self.sheets_status =
                    SheetsStatus::Failed("Google Sheets request stopped unexpectedly.".to_owned());
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        if !self.launch_update_checked {
            self.launch_update_checked = true;
            self.check_for_updates(ctx);
        }
    }

    fn send_current_time_to_sheets(&mut self, ctx: &egui::Context) {
        if self.sheets_rx.is_some() {
            return;
        }
        if !self.sheets_config.is_configured() {
            if self.pinned {
                self.toggle_pinned(ctx);
            }
            self.show_sheets_settings = true;
            return;
        }

        let elapsed = self.stopwatch.elapsed();
        if elapsed.is_zero() {
            return;
        }

        let snapshot = StopwatchSnapshot::capture(elapsed);
        self.sheets_status = SheetsStatus::Sending(sheets::rounded_minutes(elapsed));
        let config = self.sheets_config.clone();
        let (tx, rx) = mpsc::channel();
        self.sheets_rx = Some(rx);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = sheets::send_snapshot(&config, snapshot).map(SheetsActionResult::Recorded);
            let _ = tx.send(result);
            repaint_ctx.request_repaint();
        });
        ctx.request_repaint();
    }

    fn test_sheets_connection(&mut self, ctx: &egui::Context) {
        if self.sheets_rx.is_some() {
            return;
        }
        self.sheets_feedback = None;
        let draft = SheetsConfig {
            webhook_url: self.sheets_url_draft.clone(),
            secret: self.sheets_secret_draft.clone(),
        }
        .normalized();
        if let Err(message) = sheets::validate_webhook_url(&draft.webhook_url) {
            self.sheets_status = SheetsStatus::Failed(message);
            return;
        }

        self.sheets_status = SheetsStatus::Testing;
        let (tx, rx) = mpsc::channel();
        self.sheets_rx = Some(rx);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = sheets::test_connection(&draft).map(SheetsActionResult::Tested);
            let _ = tx.send(result);
            repaint_ctx.request_repaint();
        });
        ctx.request_repaint();
    }

    fn save_sheets_settings(&mut self) {
        self.sheets_feedback = None;
        let draft = SheetsConfig {
            webhook_url: self.sheets_url_draft.clone(),
            secret: self.sheets_secret_draft.clone(),
        }
        .normalized();
        if !draft.webhook_url.is_empty()
            && let Err(message) = sheets::validate_webhook_url(&draft.webhook_url)
        {
            self.sheets_status = SheetsStatus::Failed(message);
            return;
        }

        match sheets::save_config(&draft) {
            Ok(()) => {
                let cleared = draft.webhook_url.is_empty();
                self.sheets_url_draft = draft.webhook_url.clone();
                self.sheets_secret_draft = draft.secret.clone();
                self.sheets_config = draft;
                self.sheets_feedback = Some(if cleared {
                    "Cleared Google Sheets settings.".to_owned()
                } else {
                    "Saved Google Sheets settings.".to_owned()
                });
                if matches!(self.sheets_status, SheetsStatus::Failed(_)) {
                    self.sheets_status = SheetsStatus::Idle;
                }
            }
            Err(message) => {
                self.sheets_status = SheetsStatus::Failed(message);
            }
        }
    }

    fn sheets_send_button_spec(&self, compact: bool) -> (&'static str, bool, &'static str) {
        if self.sheets_rx.is_some() {
            return ("Sending…", false, "Sending stopwatch entry to Google Sheets");
        }
        if !self.sheets_config.is_configured() {
            let label = if compact {
                "Setup Sheet"
            } else {
                "Setup Sheets"
            };
            return (
                label,
                true,
                "Configure your Google Apps Script Web App URL",
            );
        }
        let has_elapsed = !self.stopwatch.elapsed().is_zero();
        let label = if compact {
            "Send to Sheet"
        } else {
            "Send to Sheets"
        };
        let tooltip = if has_elapsed {
            "Log today's date and elapsed time (in minutes) to Google Sheets"
        } else {
            "Start the stopwatch first"
        };
        (label, has_elapsed, tooltip)
    }

    fn sheets_send_row(&mut self, ui: &mut egui::Ui) {
        let (label, enabled, tooltip) = self.sheets_send_button_spec(false);
        if ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [180.0, 38.0],
                    Button::new(RichText::new(label).strong().size(14.5))
                        .fill(PANEL_LIGHT)
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(73, 98, 130)))
                        .corner_radius(CornerRadius::same(12)),
                )
                .on_hover_text(tooltip)
            })
            .inner
            .clicked()
        {
            self.send_current_time_to_sheets(ui.ctx());
        }

        match &self.sheets_status {
            SheetsStatus::Idle => {}
            SheetsStatus::Sending(minutes) => {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Sending {minutes} min to Google Sheets…"))
                        .small()
                        .color(MUTED_TEXT),
                );
            }
            SheetsStatus::Testing => {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Testing Google Sheets connection…")
                        .small()
                        .color(MUTED_TEXT),
                );
            }
            SheetsStatus::Sent(receipt) => {
                ui.add_space(6.0);
                let text = format_sent_receipt(receipt);
                ui.add(
                    egui::Label::new(RichText::new(text).small().color(SUCCESS_TEXT))
                        .wrap()
                        .selectable(true),
                );
            }
            SheetsStatus::Connected(receipt) => {
                ui.add_space(6.0);
                let text = match receipt.spreadsheet.as_deref() {
                    Some(name) if !name.is_empty() => {
                        format!("Connected to '{name}' (tab: '{}')", receipt.sheet)
                    }
                    _ => format!("Connected (tab: '{}')", receipt.sheet),
                };
                ui.add(
                    egui::Label::new(RichText::new(text).small().color(SUCCESS_TEXT))
                        .wrap()
                        .selectable(true),
                );
            }
            SheetsStatus::Failed(message) => {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(message).small().color(ERROR_TEXT))
                        .wrap()
                        .selectable(true),
                );
            }
        }
    }

    fn pinned_sheets_controls(&mut self, ui: &mut egui::Ui) {
        let (label, enabled, tooltip) = self.sheets_send_button_spec(true);
        if ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [152.0, 28.0],
                    Button::new(RichText::new(label).strong().size(12.5))
                        .fill(PANEL_LIGHT)
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(73, 98, 130)))
                        .corner_radius(CornerRadius::same(10)),
                )
                .on_hover_text(tooltip)
            })
            .inner
            .clicked()
        {
            self.send_current_time_to_sheets(ui.ctx());
        }

        ui.add_space(4.0);
        match &self.sheets_status {
            SheetsStatus::Sent(receipt) => {
                let short_text = match receipt.minutes {
                    Some(minutes) => format!("Saved {minutes} min"),
                    None => "Sent".to_owned(),
                };
                let hover = format_sent_receipt(receipt);
                ui.add(
                    egui::Label::new(RichText::new(short_text).size(11.5).color(SUCCESS_TEXT))
                        .truncate(),
                )
                .on_hover_text(hover);
            }
            SheetsStatus::Failed(message) => {
                ui.add(
                    egui::Label::new(RichText::new("Send failed").size(11.5).color(ERROR_TEXT))
                        .truncate(),
                )
                .on_hover_text(message);
            }
            SheetsStatus::Sending(_) => {
                ui.add(
                    egui::Label::new(RichText::new("Sending…").size(11.5).color(MUTED_TEXT))
                        .truncate(),
                );
            }
            SheetsStatus::Idle | SheetsStatus::Testing | SheetsStatus::Connected(_) => {}
        }
    }

    fn sheets_settings_panel(&mut self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(PANEL)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Google Sheets Setup").color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_sheets_settings = false;
                        }
                    });
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Web App URL").small().color(MUTED_TEXT));
                let url_response = ui.add(
                    egui::TextEdit::singleline(&mut self.sheets_url_draft)
                        .hint_text("https://script.google.com/macros/s/.../exec")
                        .desired_width(f32::INFINITY),
                );
                if url_response.changed() {
                    self.sheets_feedback = None;
                }

                ui.add_space(6.0);
                ui.label(
                    RichText::new("Secret Token (optional)")
                        .small()
                        .color(MUTED_TEXT),
                );
                ui.horizontal(|ui| {
                    let toggle_width = 54.0;
                    let field_width = (ui.available_width() - toggle_width - 8.0).max(120.0);
                    let secret_response = ui.add_sized(
                        [field_width, 24.0],
                        egui::TextEdit::singleline(&mut self.sheets_secret_draft)
                            .password(!self.show_sheets_secret)
                            .hint_text("Matches SHARED_SECRET in Code.gs"),
                    );
                    if secret_response.changed() {
                        self.sheets_feedback = None;
                    }
                    let toggle_label = if self.show_sheets_secret {
                        "Hide"
                    } else {
                        "Show"
                    };
                    if ui
                        .add_sized([toggle_width, 24.0], Button::new(toggle_label))
                        .clicked()
                    {
                        self.show_sheets_secret = !self.show_sheets_secret;
                    }
                });

                ui.add_space(8.0);
                let has_unsaved_changes = self.sheets_url_draft.trim()
                    != self.sheets_config.webhook_url
                    || self.sheets_secret_draft.trim() != self.sheets_config.secret;

                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save").clicked() {
                        self.save_sheets_settings();
                    }
                    if ui
                        .add_enabled(self.sheets_rx.is_none(), Button::new("Test"))
                        .on_hover_text("Verify the Web App URL and target sheet tab")
                        .clicked()
                    {
                        self.test_sheets_connection(ui.ctx());
                    }
                    if ui
                        .button("Copy Apps Script")
                        .on_hover_text("Copy Code.gs to clipboard for Extensions -> Apps Script")
                        .clicked()
                    {
                        ui.ctx().copy_text(APPS_SCRIPT_SOURCE.to_owned());
                        self.sheets_feedback =
                            Some("Copied Apps Script to clipboard.".to_owned());
                    }
                    if has_unsaved_changes {
                        ui.label(RichText::new("Unsaved changes").small().color(MUTED_TEXT));
                    }
                });

                if let Some(feedback) = &self.sheets_feedback {
                    ui.add_space(4.0);
                    ui.label(RichText::new(feedback).small().color(SUCCESS_TEXT));
                }

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "1. Open your Google Sheet -> Extensions -> Apps Script.\n\
                         2. Click Copy Apps Script, paste (Ctrl+A, Ctrl+V) into Code.gs, set TARGET_SHEET_NAME (or leave blank for first tab), and save (Ctrl+S).\n\
                         3. Deploy -> New deployment (or Manage deployments -> Edit -> Version: New version) -> Web app (Execute as: Me, Who has access: Anyone).\n\
                         4. Paste the /exec URL above, click Save, then Test.",
                    )
                    .small()
                    .color(MUTED_TEXT),
                );
            });
    }

    fn install_update(&mut self, ctx: &egui::Context) {
        if self.install_rx.is_some() {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        let Some(installer_url) = release.installer_url() else {
            self.update_state = UpdateState::Failed(
                "The latest release does not contain a Windows installer.".to_owned(),
            );
            return;
        };

        self.update_state = UpdateState::Installing;
        let (tx, rx) = mpsc::channel();
        self.install_rx = Some(rx);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(updater::download_and_launch_installer(&installer_url));
            repaint_ctx.request_repaint();
        });
        ctx.request_repaint();
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let compact = self.pinned;
        let height = if compact { 36.0 } else { 62.0 };
        let primary_width = if compact { 56.0 } else { 164.0 };
        let secondary_width = if compact { 42.0 } else { 112.0 };
        let start_icon = if self.stopwatch.is_running() {
            "Ⅱ"
        } else {
            "▶"
        };
        let start_tooltip = if self.stopwatch.is_running() {
            "Pause"
        } else if self.stopwatch.elapsed().is_zero() {
            "Start"
        } else {
            "Resume"
        };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if compact { 6.0 } else { 14.0 };
            if ui
                .add_sized(
                    [primary_width, height],
                    Button::new(RichText::new(start_icon).strong().size(if compact {
                        17.0
                    } else {
                        24.0
                    }))
                    .fill(ACCENT)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(14)),
                )
                .on_hover_text(start_tooltip)
                .clicked()
            {
                self.stopwatch.toggle();
            }

            if ui
                .add_enabled_ui(self.stopwatch.is_running(), |ui| {
                    ui.add_sized([secondary_width, height], timer_button("⚑", compact))
                        .on_hover_text("Lap")
                })
                .inner
                .clicked()
            {
                self.stopwatch.record_lap();
            }

            if ui
                .add_enabled_ui(!self.stopwatch.elapsed().is_zero(), |ui| {
                    ui.add_sized([secondary_width, height], timer_button("↻", compact))
                        .on_hover_text("Reset")
                })
                .inner
                .clicked()
            {
                self.stopwatch.reset();
            }
        });
    }

    fn update_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        match &self.update_state {
            UpdateState::Checking => {
                ui.label(RichText::new("Checking for updates…").small());
            }
            UpdateState::Available => {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(TEXT, "A Stopwatch update is available.");
                    if ui.button("Install update").clicked() {
                        self.install_update(ui.ctx());
                    }
                });
            }
            UpdateState::Installing => {
                ui.label(RichText::new("Downloading update…").small());
            }
            UpdateState::Failed(message) => {
                ui.colored_label(TEXT, message);
                if ui.button("Retry update check").clicked() {
                    self.check_for_updates(ui.ctx());
                }
            }
            UpdateState::Current => {
                ui.label(RichText::new("Stopwatch is up to date.").small());
            }
            UpdateState::Idle | UpdateState::Unavailable => {}
        }
    }
}

fn format_sent_receipt(receipt: &SendReceipt) -> String {
    let mins = receipt
        .minutes
        .map_or_else(|| "time".to_owned(), |m| format!("{m} min"));
    let verb = if receipt.updated == Some(true) {
        "Updated"
    } else {
        "Logged"
    };
    let date_part = receipt
        .date
        .as_deref()
        .map_or_else(String::new, |date| format!(" for {date}"));
    let row_suffix = receipt
        .row
        .map_or_else(String::new, |row| format!(" (row {row})"));
    format!(
        "{verb} {mins}{date_part} on '{}'{row_suffix}",
        receipt.sheet
    )
}

fn timer_button(label: &str, compact: bool) -> Button<'_> {
    Button::new(
        RichText::new(label)
            .strong()
            .size(if compact { 18.0 } else { 24.0 }),
    )
    .fill(PANEL_LIGHT)
    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(73, 98, 130)))
    .corner_radius(CornerRadius::same(14))
}

impl eframe::App for StopwatchApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.position_pinned_window(ctx, frame);
        self.handle_background_work(ctx);
        if self.stopwatch.is_running() {
            ctx.request_repaint_after(if self.pinned {
                PINNED_TIMER_REFRESH_INTERVAL
            } else {
                TIMER_REFRESH_INTERVAL
            });
        }
        if self.update_rx.is_some() || self.install_rx.is_some() || self.sheets_rx.is_some() {
            ctx.request_repaint_after(BACKGROUND_POLL_INTERVAL);
        }

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(Margin::symmetric(if self.pinned { 8 } else { 24 }, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.pinned {
                        let drag = ui
                            .add(
                                timer_button("⠿", true)
                                    .min_size(Vec2::new(38.0, 30.0))
                                    .sense(egui::Sense::drag()),
                            )
                            .on_hover_text("Move window");
                        if drag.drag_started() {
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                    }
                    if !self.pinned {
                        ui.heading(RichText::new(APP_NAME).color(TEXT).size(26.0).strong());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.pinned
                            && ui
                                .add(timer_button("×", true).min_size(Vec2::new(30.0, 30.0)))
                                .on_hover_text("Close")
                                .clicked()
                        {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        let pin_icon = if self.pinned { "⊠" } else { "⌖" };
                        let pin_tooltip = if self.pinned {
                            "Unpin window"
                        } else {
                            "Pin to top right"
                        };
                        if ui
                            .add(timer_button(pin_icon, true).min_size(Vec2::new(
                                if self.pinned { 38.0 } else { 42.0 },
                                if self.pinned { 30.0 } else { 36.0 },
                            )))
                            .on_hover_text(pin_tooltip)
                            .clicked()
                        {
                            self.toggle_pinned(ctx);
                        }
                        if !self.pinned {
                            let mut sheets_button = Button::new(
                                RichText::new("Sheets").strong().size(14.0),
                            )
                            .fill(PANEL_LIGHT)
                            .stroke(if self.show_sheets_settings {
                                Stroke::new(1.5_f32, ACCENT)
                            } else {
                                Stroke::new(1.0_f32, Color32::from_rgb(73, 98, 130))
                            })
                            .corner_radius(CornerRadius::same(14))
                            .min_size(Vec2::new(72.0, 36.0));
                            if self.sheets_config.is_configured() && !self.show_sheets_settings {
                                sheets_button = sheets_button.stroke(Stroke::new(
                                    1.0_f32,
                                    Color32::from_rgb(95, 136, 175),
                                ));
                            }
                            if ui
                                .add(sheets_button)
                                .on_hover_text("Configure Google Sheets integration")
                                .clicked()
                            {
                                self.show_sheets_settings = !self.show_sheets_settings;
                            }
                        }
                    });
                });

                ui.add_space(if self.pinned { 4.0 } else { 22.0 });

                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.stopwatch.formatted())
                            .monospace()
                            .color(TEXT)
                            .size(if self.pinned { 23.0 } else { 64.0 }),
                    );
                    ui.add_space(if self.pinned { 6.0 } else { 20.0 });
                    self.controls(ui);
                    if self.pinned {
                        ui.add_space(6.0);
                        self.pinned_sheets_controls(ui);
                    } else {
                        ui.add_space(14.0);
                        self.sheets_send_row(ui);
                    }
                });

                if !self.pinned {
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .id_salt("main_lower_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.show_sheets_settings {
                                self.sheets_settings_panel(ui);
                                ui.add_space(12.0);
                            }

                            if !self.stopwatch.laps().is_empty() {
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.heading(RichText::new("Laps").color(TEXT));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.toggle_value(&mut self.show_laps, "Show");
                                    });
                                });
                                if self.show_laps {
                                    Frame::new()
                                        .fill(PANEL)
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(Margin::same(10))
                                        .show(ui, |ui| {
                                            egui::ScrollArea::vertical()
                                                .id_salt("laps_scroll")
                                                .max_height(130.0)
                                                .show(ui, |ui| {
                                                    for (index, lap) in
                                                        self.stopwatch.laps().iter().enumerate().rev()
                                                    {
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                RichText::new(format!(
                                                                    "Lap {}",
                                                                    index + 1
                                                                ))
                                                                .color(MUTED_TEXT),
                                                            );
                                                            ui.with_layout(
                                                                Layout::right_to_left(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.monospace(
                                                                        RichText::new(
                                                                            Stopwatch::format_duration(*lap),
                                                                        )
                                                                        .color(TEXT),
                                                                    );
                                                                },
                                                            );
                                                        });
                                                    }
                                                });
                                        });
                                }
                            }
                            self.update_panel(ui);
                        });
                }
            });
    }
}
