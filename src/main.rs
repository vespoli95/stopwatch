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
const PINNED_SIZE: [f32; 2] = [256.0, 150.0];
const PINNED_MARGIN: i8 = 12;
const PINNED_CORNER_RADIUS: u8 = 18;
const BORDER_STROKE_COLOR: Color32 = Color32::from_rgb(73, 98, 130);
const CHROME_STROKE_COLOR: Color32 = Color32::from_rgb(63, 86, 116);
const UNPINNED_SIZE: [f32; 2] = [560.0, 520.0];
const TIMER_REFRESH_INTERVAL: Duration = Duration::from_millis(47);
const PINNED_TIMER_REFRESH_INTERVAL: Duration = Duration::from_millis(63);
const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_millis(500);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size(UNPINNED_SIZE)
            .with_min_inner_size([400.0, 300.0])
            .with_resizable(true)
            .with_transparent(true),
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
    pending_pin_position_frames: u8,
    background_drag_eligible: bool,
    background_drag_started: bool,
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
            pending_pin_position_frames: 0,
            background_drag_eligible: false,
            background_drag_started: false,
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
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
        visuals.widgets.inactive.bg_fill = PANEL_LIGHT;
        visuals.widgets.inactive.weak_bg_fill = PANEL;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(57, 82, 116);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(57, 82, 116);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        visuals.widgets.active.bg_fill = Color32::from_rgb(112, 151, 190);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
        visuals.widgets.active.corner_radius = CornerRadius::same(6);
        visuals.widgets.open.bg_fill = PANEL_LIGHT;
        visuals.widgets.open.corner_radius = CornerRadius::same(6);
        ctx.set_visuals(visuals);
        ctx.style_mut(|style| {
            style.interaction.selectable_labels = false;
        });
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
        let [width, height] = if self.pinned {
            PINNED_SIZE
        } else {
            UNPINNED_SIZE
        };
        ctx.send_viewport_cmd(ViewportCommand::Resizable(!self.pinned));
        if self.pinned {
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(width, height)));
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(Vec2::new(width, height)));
        } else {
            let monitor_size = ctx
                .input(|input| input.viewport().monitor_size)
                .unwrap_or(Vec2::new(4096.0, 2160.0));
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(monitor_size));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(400.0, 300.0)));
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
                    .map_or(PINNED_SIZE[0], |rect| rect.width()),
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
            let label = if compact { "…" } else { "Sending…" };
            return (label, false, "Sending stopwatch entry to Google Sheets");
        }
        if !self.sheets_config.is_configured() {
            let label = if compact { "Setup" } else { "Setup Sheets" };
            return (
                label,
                true,
                "Configure your Google Apps Script Web App URL",
            );
        }
        let has_elapsed = !self.stopwatch.elapsed().is_zero();
        let label = if compact { "Sheet" } else { "Send to Sheets" };
        let tooltip = if has_elapsed {
            "Log today's date and elapsed time (in minutes) to Google Sheets"
        } else if compact {
            "Send to Sheets (start the stopwatch first)"
        } else {
            "Start the stopwatch first"
        };
        (label, has_elapsed, tooltip)
    }

    fn sheets_status_row(&self, ui: &mut egui::Ui) {
        match &self.sheets_status {
            SheetsStatus::Idle => {}
            SheetsStatus::Sending(minutes) => {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("Sending {minutes} min to Google Sheets…"))
                        .small()
                        .color(MUTED_TEXT),
                );
            }
            SheetsStatus::Testing => {
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Testing Google Sheets connection…")
                        .small()
                        .color(MUTED_TEXT),
                );
            }
            SheetsStatus::Sent(receipt) => {
                ui.add_space(10.0);
                let text = format_sent_receipt(receipt);
                ui.add(
                    egui::Label::new(RichText::new(text).small().color(SUCCESS_TEXT))
                        .wrap()
                        .selectable(true),
                );
            }
            SheetsStatus::Connected(receipt) => {
                ui.add_space(10.0);
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
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(RichText::new(message).small().color(ERROR_TEXT))
                        .wrap()
                        .selectable(true),
                );
            }
        }
    }

    fn pinned_sheets_status(&self, ui: &mut egui::Ui) {
        match &self.sheets_status {
            SheetsStatus::Sent(receipt) => {
                let short_text = match receipt.minutes {
                    Some(minutes) => format!("Saved {minutes}m"),
                    None => "Sent".to_owned(),
                };
                let hover = format_sent_receipt(receipt);
                ui.add(
                    egui::Label::new(RichText::new(short_text).size(13.0).color(SUCCESS_TEXT))
                        .truncate(),
                )
                .on_hover_text(hover);
            }
            SheetsStatus::Failed(message) => {
                ui.add(
                    egui::Label::new(RichText::new("Failed").size(13.0).color(ERROR_TEXT))
                        .truncate(),
                )
                .on_hover_text(message);
            }
            SheetsStatus::Sending(_) => {
                ui.add(
                    egui::Label::new(RichText::new("Sending…").size(13.0).color(MUTED_TEXT))
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
        let start_icon = if self.stopwatch.is_running() {
            Icon::Pause
        } else {
            Icon::Play
        };
        let start_tooltip = if self.stopwatch.is_running() {
            "Pause"
        } else if self.stopwatch.elapsed().is_zero() {
            "Start"
        } else {
            "Resume"
        };

        if self.pinned {
            let row_width = PINNED_SIZE[0] - 2.0 * f32::from(PINNED_MARGIN);
            ui.allocate_ui_with_layout(
                Vec2::new(row_width, 38.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.spacing_mut().button_padding = Vec2::new(4.0, 1.0);
                    if icon_button(
                        ui,
                        Vec2::new(76.0, 38.0),
                        start_icon,
                        18.0,
                        ACCENT,
                        Stroke::NONE,
                        CornerRadius::same(11),
                    )
                    .on_hover_text(start_tooltip)
                    .clicked()
                    {
                        self.stopwatch.toggle();
                    }

                    if ui
                        .add_enabled_ui(!self.stopwatch.elapsed().is_zero(), |ui| {
                            icon_button(
                                ui,
                                Vec2::new(56.0, 38.0),
                                Icon::Reset,
                                18.0,
                                PANEL_LIGHT,
                                Stroke::new(1.0_f32, BORDER_STROKE_COLOR),
                                CornerRadius::same(11),
                            )
                            .on_hover_text("Reset")
                        })
                        .inner
                        .clicked()
                    {
                        self.stopwatch.reset();
                    }

                    let (label, enabled, tooltip) = self.sheets_send_button_spec(true);
                    if ui
                        .add_enabled_ui(enabled, |ui| {
                            ui.add_sized(
                                [84.0, 38.0],
                                Button::new(RichText::new(label).strong().size(14.5))
                                    .fill(PANEL_LIGHT)
                                    .stroke(Stroke::new(1.0_f32, BORDER_STROKE_COLOR))
                                    .corner_radius(CornerRadius::same(11)),
                            )
                            .on_hover_text(tooltip)
                        })
                        .inner
                        .clicked()
                    {
                        self.send_current_time_to_sheets(ui.ctx());
                    }
                },
            );
            return;
        }

        ui.allocate_ui_with_layout(
            Vec2::new(400.0, 54.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if icon_button(
                    ui,
                    Vec2::new(132.0, 54.0),
                    start_icon,
                    20.0,
                    ACCENT,
                    Stroke::NONE,
                    CornerRadius::same(14),
                )
                .on_hover_text(start_tooltip)
                .clicked()
                {
                    self.stopwatch.toggle();
                }

                if ui
                    .add_enabled_ui(!self.stopwatch.elapsed().is_zero(), |ui| {
                        icon_button(
                            ui,
                            Vec2::new(88.0, 54.0),
                            Icon::Reset,
                            20.0,
                            PANEL_LIGHT,
                            Stroke::new(1.0_f32, BORDER_STROKE_COLOR),
                            CornerRadius::same(14),
                        )
                        .on_hover_text("Reset")
                    })
                    .inner
                    .clicked()
                {
                    self.stopwatch.reset();
                }

                let (label, enabled, tooltip) = self.sheets_send_button_spec(false);
                if ui
                    .add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            [156.0, 54.0],
                            Button::new(RichText::new(label).strong().size(15.0))
                                .fill(PANEL_LIGHT)
                                .stroke(Stroke::new(1.0_f32, BORDER_STROKE_COLOR))
                                .corner_radius(CornerRadius::same(14)),
                        )
                        .on_hover_text(tooltip)
                    })
                    .inner
                    .clicked()
                {
                    self.send_current_time_to_sheets(ui.ctx());
                }
            },
        );
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

#[derive(Clone, Copy)]
enum Icon {
    Play,
    Pause,
    Reset,
    Expand,
    Pin,
    Close,
}

fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, size: f32, color: Color32) {
    let c = rect.center();
    let r = size * 0.5;
    match icon {
        Icon::Play => {
            let points = vec![
                egui::pos2(c.x - r * 0.55, c.y - r * 0.85),
                egui::pos2(c.x - r * 0.55, c.y + r * 0.85),
                egui::pos2(c.x + r * 0.90, c.y),
            ];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Pause => {
            let bar_w = (size * 0.28).max(2.0);
            let bar_h = size * 0.88;
            let gap = (size * 0.24).max(2.0);
            let offset = (bar_w + gap) * 0.5;
            let bar_size = Vec2::new(bar_w, bar_h);
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x - offset, c.y), bar_size),
                CornerRadius::same(1),
                color,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x + offset, c.y), bar_size),
                CornerRadius::same(1),
                color,
            );
        }
        Icon::Reset => {
            let radius = r * 0.78;
            let stroke_w = (size * 0.13).clamp(1.6, 2.6);
            let start_angle = 0.18 * std::f32::consts::PI;
            let end_angle = 1.76 * std::f32::consts::PI;
            let segments = 18;
            let points: Vec<egui::Pos2> = (0..=segments)
                .map(|i| {
                    let t = i as f32 / segments as f32;
                    let angle = start_angle + t * (end_angle - start_angle);
                    egui::pos2(c.x + angle.cos() * radius, c.y + angle.sin() * radius)
                })
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(stroke_w, color)));

            let radial = egui::vec2(end_angle.cos(), end_angle.sin());
            let tangent = egui::vec2(-end_angle.sin(), end_angle.cos());
            let head = (size * 0.28).max(3.2);
            let end_pt = c + radial * radius;
            let tip = end_pt + tangent * (head * 0.7);
            let base = end_pt - tangent * (head * 0.3);
            let arrow = vec![
                tip,
                base + radial * (head * 0.7),
                base - radial * (head * 0.7),
            ];
            painter.add(egui::Shape::convex_polygon(arrow, color, Stroke::NONE));
        }
        Icon::Expand => {
            let half = r * 0.80;
            let arm = r * 0.56;
            let stroke = Stroke::new((size * 0.14).clamp(1.5, 2.2), color);
            let tr = egui::pos2(c.x + half, c.y - half);
            painter.line_segment([egui::pos2(c.x + r * 0.12, c.y - r * 0.12), tr], stroke);
            painter.line_segment([tr, egui::pos2(tr.x - arm, tr.y)], stroke);
            painter.line_segment([tr, egui::pos2(tr.x, tr.y + arm)], stroke);

            let bl = egui::pos2(c.x - half, c.y + half);
            painter.line_segment([egui::pos2(c.x - r * 0.12, c.y + r * 0.12), bl], stroke);
            painter.line_segment([bl, egui::pos2(bl.x + arm, bl.y)], stroke);
            painter.line_segment([bl, egui::pos2(bl.x, bl.y - arm)], stroke);
        }
        Icon::Pin => {
            let outer = egui::Rect::from_center_size(c, Vec2::new(size * 1.05, size * 0.85));
            let stroke = Stroke::new((size * 0.11).clamp(1.5, 2.2), color);
            painter.rect_stroke(
                outer,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            let inset = stroke.width + 1.5;
            let inner_w = (outer.width() - inset * 2.0) * 0.46;
            let inner_h = (outer.height() - inset * 2.0) * 0.46;
            let badge = egui::Rect::from_min_size(
                egui::pos2(outer.max.x - inset - inner_w, outer.min.y + inset),
                Vec2::new(inner_w, inner_h),
            );
            painter.rect_filled(badge, CornerRadius::same(1), color);
        }
        Icon::Close => {
            let half = r * 0.72;
            let stroke = Stroke::new((size * 0.15).clamp(1.5, 2.2), color);
            painter.line_segment(
                [
                    egui::pos2(c.x - half, c.y - half),
                    egui::pos2(c.x + half, c.y + half),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + half, c.y - half),
                    egui::pos2(c.x - half, c.y + half),
                ],
                stroke,
            );
        }
    }
}

fn icon_button(
    ui: &mut egui::Ui,
    size: Vec2,
    icon: Icon,
    icon_size: f32,
    fill: Color32,
    stroke: Stroke,
    corner_radius: CornerRadius,
) -> egui::Response {
    let response = ui.add_sized(
        size,
        Button::new("")
            .min_size(size)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(corner_radius)
            .sense(egui::Sense::click()),
    );
    let icon_color = ui.style().interact(&response).fg_stroke.color;
    paint_icon(ui.painter(), response.rect, icon, icon_size, icon_color);
    response
}

impl StopwatchApp {
    fn handle_background_drag(&mut self, ctx: &egui::Context, ui: &egui::Ui, rect: egui::Rect) {
        let bg_probe = ui.interact(
            rect,
            egui::Id::new("window_background_drag"),
            egui::Sense::click(),
        );
        let (primary_pressed, primary_down, pointer_moved) = ctx.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_down(egui::PointerButton::Primary),
                i.pointer.delta() != Vec2::ZERO,
            )
        });
        if !primary_down {
            self.background_drag_eligible = false;
            self.background_drag_started = false;
        } else {
            if primary_pressed
                && bg_probe.is_pointer_button_down_on()
                && ctx.dragged_id().is_none()
            {
                self.background_drag_eligible = true;
            }
            if self.background_drag_eligible
                && !self.background_drag_started
                && ctx.dragged_id().is_none()
                && pointer_moved
            {
                self.background_drag_started = true;
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        }
    }

    fn show_ui(&mut self, ctx: &egui::Context) -> egui::Rect {
        let mut used_rect = egui::Rect::NOTHING;
        if self.pinned {
            let screen_rect = ctx.screen_rect();
            let inner_rect = screen_rect.shrink(f32::from(PINNED_MARGIN));
            egui::CentralPanel::default()
                .frame(Frame::NONE)
                .show(ctx, |ui| {
                    ui.painter().rect(
                        screen_rect,
                        CornerRadius::same(PINNED_CORNER_RADIUS),
                        BACKGROUND,
                        Stroke::new(1.0_f32, BORDER_STROKE_COLOR),
                        egui::StrokeKind::Inside,
                    );
                    self.handle_background_drag(ctx, ui, screen_rect);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
                        used_rect = self.show_panel_contents(ctx, ui);
                    });
                });
        } else {
            let screen_rect = ctx.screen_rect();
            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(BACKGROUND)
                        .inner_margin(Margin::symmetric(24, 8)),
                )
                .show(ctx, |ui| {
                    self.handle_background_drag(ctx, ui, screen_rect);
                    used_rect = self.show_panel_contents(ctx, ui);
                });
        }
        used_rect
    }

    fn show_panel_contents(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> egui::Rect {
                ui.horizontal(|ui| {
                    if self.pinned {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.spacing_mut().button_padding = Vec2::ZERO;
                    } else {
                        ui.heading(RichText::new(APP_NAME).color(TEXT).size(26.0).strong());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.pinned {
                            if icon_button(
                                ui,
                                Vec2::new(30.0, 28.0),
                                Icon::Close,
                                14.0,
                                PANEL,
                                Stroke::new(1.0_f32, CHROME_STROKE_COLOR),
                                CornerRadius::same(8),
                            )
                            .on_hover_text("Close")
                            .clicked()
                            {
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                            if icon_button(
                                ui,
                                Vec2::new(30.0, 28.0),
                                Icon::Expand,
                                14.5,
                                PANEL,
                                Stroke::new(1.0_f32, CHROME_STROKE_COLOR),
                                CornerRadius::same(8),
                            )
                            .on_hover_text("Unpin window")
                            .clicked()
                            {
                                self.toggle_pinned(ctx);
                            }
                            let status_width = ui.available_width().max(0.0);
                            if status_width > 0.0 {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(status_width, 28.0),
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        self.pinned_sheets_status(ui);
                                    },
                                );
                            }
                        } else {
                            if icon_button(
                                ui,
                                Vec2::new(42.0, 36.0),
                                Icon::Pin,
                                18.0,
                                PANEL_LIGHT,
                                Stroke::new(1.0_f32, BORDER_STROKE_COLOR),
                                CornerRadius::same(14),
                            )
                            .on_hover_text("Pin to top right")
                            .clicked()
                            {
                                self.toggle_pinned(ctx);
                            }
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

                ui.add_space(if self.pinned { 6.0 } else { 22.0 });

                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.stopwatch.formatted())
                            .monospace()
                            .color(TEXT)
                            .size(if self.pinned { 32.0 } else { 64.0 }),
                    );
                    ui.add_space(if self.pinned { 9.0 } else { 20.0 });
                    self.controls(ui);
                    if !self.pinned {
                        self.sheets_status_row(ui);
                    }
                });

                if !self.pinned {
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical()
                        .id_salt("main_lower_scroll")
                        .drag_to_scroll(false)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.show_sheets_settings {
                                self.sheets_settings_panel(ui);
                                ui.add_space(12.0);
                            }

                            self.update_panel(ui);
                        });
                }

                ui.min_rect()
    }
}

impl eframe::App for StopwatchApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.pinned {
            egui::Rgba::TRANSPARENT.to_array()
        } else {
            egui::Rgba::from(BACKGROUND).to_array()
        }
    }

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

        let _ = self.show_ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App as _;

    fn run_layout(app: &mut StopwatchApp, size: [f32; 2]) -> (egui::Rect, egui::FullOutput) {
        let ctx = egui::Context::default();
        StopwatchApp::apply_theme(&ctx);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::from(size),
            )),
            ..Default::default()
        };
        let mut used_rect = egui::Rect::NOTHING;
        let mut full_output = egui::FullOutput::default();
        for _ in 0..2 {
            full_output = ctx.run(raw_input.clone(), |ctx| {
                used_rect = app.show_ui(ctx);
            });
        }
        (used_rect, full_output)
    }

    fn run_pinned_layout(app: &mut StopwatchApp) -> (egui::Rect, egui::FullOutput) {
        app.pinned = true;
        run_layout(app, PINNED_SIZE)
    }

    fn find_pinned_panel_rect_in_shape(shape: &egui::Shape) -> Option<&egui::epaint::RectShape> {
        match shape {
            egui::Shape::Rect(rect_shape)
                if rect_shape.fill == BACKGROUND
                    && rect_shape.corner_radius == CornerRadius::same(PINNED_CORNER_RADIUS) =>
            {
                Some(rect_shape)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(find_pinned_panel_rect_in_shape),
            _ => None,
        }
    }

    fn find_pinned_panel_rect(
        shapes: &[egui::epaint::ClippedShape],
    ) -> Option<&egui::epaint::RectShape> {
        shapes
            .iter()
            .find_map(|clipped| find_pinned_panel_rect_in_shape(&clipped.shape))
    }

    fn find_rect_in_shape(
        shape: &egui::Shape,
        predicate: &impl Fn(&egui::epaint::RectShape) -> bool,
    ) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Rect(rect_shape) if predicate(rect_shape) => Some(rect_shape.rect),
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .find_map(|s| find_rect_in_shape(s, predicate)),
            _ => None,
        }
    }

    fn find_rect(
        shapes: &[egui::epaint::ClippedShape],
        predicate: impl Fn(&egui::epaint::RectShape) -> bool,
    ) -> Option<egui::Rect> {
        shapes
            .iter()
            .find_map(|clipped| find_rect_in_shape(&clipped.shape, &predicate))
    }

    fn simulate_drag(
        app: &mut StopwatchApp,
        size: [f32; 2],
        start: egui::Pos2,
        delta: Vec2,
    ) -> (Vec<ViewportCommand>, Vec<egui::epaint::ClippedShape>) {
        let ctx = egui::Context::default();
        StopwatchApp::apply_theme(&ctx);
        let screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::from(size),
        ));
        let mut run_frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect,
                events,
                ..Default::default()
            };
            ctx.run(input, |ctx| {
                let _ = app.show_ui(ctx);
            })
        };

        let _ = run_frame(vec![]);
        let hover_output = run_frame(vec![egui::Event::PointerMoved(start)]);
        let press_output = run_frame(vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let move_output = run_frame(vec![egui::Event::PointerMoved(start + delta)]);

        let mut commands = Vec::new();
        for output in [press_output, move_output] {
            if let Some(viewport) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                commands.extend(viewport.commands.iter().cloned());
            }
        }
        (commands, hover_output.shapes)
    }

    fn has_start_drag(commands: &[ViewportCommand]) -> bool {
        commands
            .iter()
            .any(|cmd| matches!(cmd, ViewportCommand::StartDrag))
    }

    #[test]
    fn sheets_send_button_spec_matches_compact_and_full_states() {
        let mut app = StopwatchApp::default();
        assert_eq!(
            app.sheets_send_button_spec(true),
            ("Setup", true, "Configure your Google Apps Script Web App URL")
        );
        assert_eq!(
            app.sheets_send_button_spec(false),
            (
                "Setup Sheets",
                true,
                "Configure your Google Apps Script Web App URL"
            )
        );

        app.sheets_config.webhook_url = "https://script.google.com/macros/s/test/exec".to_owned();
        assert_eq!(
            app.sheets_send_button_spec(true),
            (
                "Sheet",
                false,
                "Send to Sheets (start the stopwatch first)"
            )
        );
        assert_eq!(
            app.sheets_send_button_spec(false),
            ("Send to Sheets", false, "Start the stopwatch first")
        );

        app.stopwatch.toggle();
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(
            app.sheets_send_button_spec(true),
            (
                "Sheet",
                true,
                "Log today's date and elapsed time (in minutes) to Google Sheets"
            )
        );
        assert_eq!(
            app.sheets_send_button_spec(false),
            (
                "Send to Sheets",
                true,
                "Log today's date and elapsed time (in minutes) to Google Sheets"
            )
        );

        let (_tx, rx) = mpsc::channel();
        app.sheets_rx = Some(rx);
        assert_eq!(
            app.sheets_send_button_spec(true),
            ("…", false, "Sending stopwatch entry to Google Sheets")
        );
        assert_eq!(
            app.sheets_send_button_spec(false),
            ("Sending…", false, "Sending stopwatch entry to Google Sheets")
        );
    }

    #[test]
    fn pinned_layout_fits_within_pinned_size_across_states() {
        let mut unconfigured = StopwatchApp::default();
        let mut configured_running = StopwatchApp::default();
        configured_running.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        configured_running.stopwatch.toggle();
        std::thread::sleep(Duration::from_millis(2));

        let mut sending = StopwatchApp::default();
        sending.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        let (_tx, rx) = mpsc::channel();
        sending.sheets_rx = Some(rx);
        sending.sheets_status = SheetsStatus::Sending(12);

        let mut sent = StopwatchApp::default();
        sent.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        sent.sheets_status = SheetsStatus::Sent(SendReceipt {
            spreadsheet: Some("Time Log".to_owned()),
            sheet: "Sheet1".to_owned(),
            date: Some("2026-11-22".to_owned()),
            minutes: Some(125),
            row: Some(4),
            updated: Some(false),
        });

        let mut failed = StopwatchApp::default();
        failed.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        failed.sheets_status = SheetsStatus::Failed("Network timeout".to_owned());

        let margin = f32::from(PINNED_MARGIN);
        for (name, app) in [
            ("unconfigured", &mut unconfigured),
            ("configured_running", &mut configured_running),
            ("sending", &mut sending),
            ("sent", &mut sent),
            ("failed", &mut failed),
        ] {
            let (used_rect, full_output) = run_pinned_layout(app);
            assert_eq!(
                app.clear_color(&egui::Visuals::dark())[3],
                0.0,
                "{name}: pinned mode must use transparent clear_color"
            );
            let panel_rect = find_pinned_panel_rect(&full_output.shapes)
                .expect("pinned panel rect shape with rounded corners must be emitted");
            assert!(
                panel_rect.stroke.width >= 1.0,
                "{name}: pinned panel border stroke must be visible: {:?}",
                panel_rect.stroke
            );
            let stroke_outset = match panel_rect.stroke_kind {
                egui::StrokeKind::Inside => 0.0,
                egui::StrokeKind::Middle => panel_rect.stroke.width * 0.5,
                egui::StrokeKind::Outside => panel_rect.stroke.width,
            };
            assert!(
                panel_rect.rect.min.x - stroke_outset >= -0.01
                    && panel_rect.rect.min.y - stroke_outset >= -0.01
                    && panel_rect.rect.max.x + stroke_outset <= PINNED_SIZE[0] + 0.01
                    && panel_rect.rect.max.y + stroke_outset <= PINNED_SIZE[1] + 0.01,
                "{name}: pinned border clipped outside window bounds: rect={:?}, stroke_kind={:?}, stroke_width={}",
                panel_rect.rect,
                panel_rect.stroke_kind,
                panel_rect.stroke.width
            );
            assert!(
                (panel_rect.rect.width() - PINNED_SIZE[0]).abs() <= 0.5
                    && (panel_rect.rect.height() - PINNED_SIZE[1]).abs() <= 0.5,
                "{name}: pinned panel rect did not fill PINNED_SIZE: rect={:?}",
                panel_rect.rect
            );
            let used_height = used_rect.max.y + margin;
            assert!(
                used_rect.max.x + margin <= PINNED_SIZE[0] + 0.5,
                "{name}: width overflowed pinned bounds: used_rect={used_rect:?}"
            );
            assert!(
                used_height <= PINNED_SIZE[1] + 0.5,
                "{name}: height overflowed pinned bounds: used_rect={used_rect:?}"
            );
            assert!(
                used_height >= PINNED_SIZE[1] - 6.0,
                "{name}: excessive vertical slack (used_height={used_height}, target={}): used_rect={used_rect:?}",
                PINNED_SIZE[1]
            );
        }
    }

    #[test]
    fn unpinned_layout_fits_within_unpinned_size() {
        let mut unconfigured = StopwatchApp::default();

        let mut running = StopwatchApp::default();
        running.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        running.stopwatch.toggle();
        std::thread::sleep(Duration::from_millis(2));

        let mut sent = StopwatchApp::default();
        sent.sheets_config.webhook_url =
            "https://script.google.com/macros/s/test/exec".to_owned();
        sent.sheets_status = SheetsStatus::Sent(SendReceipt {
            spreadsheet: Some("Time Log".to_owned()),
            sheet: "Sheet1".to_owned(),
            date: Some("2026-11-22".to_owned()),
            minutes: Some(45),
            row: Some(2),
            updated: Some(true),
        });

        for (name, app) in [
            ("unconfigured", &mut unconfigured),
            ("running", &mut running),
            ("sent", &mut sent),
        ] {
            app.pinned = false;
            assert_eq!(
                app.clear_color(&egui::Visuals::dark())[3],
                1.0,
                "{name}: unpinned mode must use opaque clear_color"
            );
            let (used_rect, _) = run_layout(app, UNPINNED_SIZE);
            assert!(
                used_rect.max.x + 24.0 <= UNPINNED_SIZE[0] + 0.5,
                "{name}: width overflowed unpinned bounds: used_rect={used_rect:?}"
            );
        }
    }

    #[test]
    fn background_whitespace_drag_moves_window_without_stealing_widget_interactions() {
        let mut pinned_top_left = StopwatchApp {
            pinned: true,
            ..Default::default()
        };
        let (cmds, _) = simulate_drag(
            &mut pinned_top_left,
            PINNED_SIZE,
            egui::pos2(25.0, 23.0),
            Vec2::new(12.0, 8.0),
        );
        assert!(
            has_start_drag(&cmds),
            "pinned top-left whitespace drag (former Move spot) should emit StartDrag: {cmds:?}"
        );

        let mut pinned_top = StopwatchApp {
            pinned: true,
            ..Default::default()
        };
        let (cmds, _) = simulate_drag(
            &mut pinned_top,
            PINNED_SIZE,
            egui::pos2(96.0, 23.0),
            Vec2::new(12.0, 8.0),
        );
        assert!(
            has_start_drag(&cmds),
            "pinned top-bar whitespace drag should emit StartDrag: {cmds:?}"
        );

        let mut pinned_timer = StopwatchApp {
            pinned: true,
            ..Default::default()
        };
        let (cmds, _) = simulate_drag(
            &mut pinned_timer,
            PINNED_SIZE,
            egui::pos2(112.0, 62.0),
            Vec2::new(10.0, 6.0),
        );
        assert!(
            has_start_drag(&cmds),
            "pinned timer label drag should emit StartDrag: {cmds:?}"
        );

        let mut pinned_btn = StopwatchApp {
            pinned: true,
            ..Default::default()
        };
        let (_, output) = run_pinned_layout(&mut pinned_btn);
        let accent_rect = find_rect(&output.shapes, |r| r.fill == ACCENT)
            .expect("pinned start button rect must exist");
        let (cmds, _) = simulate_drag(
            &mut pinned_btn,
            PINNED_SIZE,
            accent_rect.center(),
            Vec2::new(8.0, 4.0),
        );
        assert!(
            !has_start_drag(&cmds),
            "dragging on pinned Start/Pause button must not emit StartDrag: {cmds:?}"
        );

        let mut unpinned_upper = StopwatchApp::default();
        let (cmds, _) = simulate_drag(
            &mut unpinned_upper,
            UNPINNED_SIZE,
            egui::pos2(40.0, 110.0),
            Vec2::new(14.0, 10.0),
        );
        assert!(
            has_start_drag(&cmds),
            "unpinned upper whitespace drag should emit StartDrag: {cmds:?}"
        );

        let mut unpinned_lower = StopwatchApp::default();
        let (cmds, _) = simulate_drag(
            &mut unpinned_lower,
            UNPINNED_SIZE,
            egui::pos2(280.0, 380.0),
            Vec2::new(14.0, 10.0),
        );
        assert!(
            has_start_drag(&cmds),
            "unpinned lower ScrollArea whitespace drag should emit StartDrag: {cmds:?}"
        );

        let mut unpinned_btn = StopwatchApp::default();
        let (_, output) = run_layout(&mut unpinned_btn, UNPINNED_SIZE);
        let accent_rect = find_rect(&output.shapes, |r| r.fill == ACCENT)
            .expect("unpinned start button rect must exist");
        let (cmds, _) = simulate_drag(
            &mut unpinned_btn,
            UNPINNED_SIZE,
            accent_rect.center(),
            Vec2::new(10.0, 6.0),
        );
        assert!(
            !has_start_drag(&cmds),
            "dragging on unpinned Start/Pause button must not emit StartDrag: {cmds:?}"
        );

        let mut unpinned_input = StopwatchApp {
            show_sheets_settings: true,
            ..Default::default()
        };
        let (_, output) = run_layout(&mut unpinned_input, UNPINNED_SIZE);
        let text_edit_rect = find_rect(&output.shapes, |r| {
            r.fill == PANEL_LIGHT
                && r.rect.width() > 200.0
                && r.rect.height() < 40.0
                && r.rect.min.y > 220.0
        })
        .expect("Web App URL TextEdit rect must exist");
        let (cmds, _) = simulate_drag(
            &mut unpinned_input,
            UNPINNED_SIZE,
            text_edit_rect.center(),
            Vec2::new(16.0, 0.0),
        );
        assert!(
            !has_start_drag(&cmds),
            "dragging inside TextEdit must not emit StartDrag: {cmds:?}"
        );

        let mut stationary_click = StopwatchApp {
            pinned: true,
            ..Default::default()
        };
        let (cmds, _) = simulate_drag(
            &mut stationary_click,
            PINNED_SIZE,
            egui::pos2(96.0, 23.0),
            Vec2::ZERO,
        );
        assert!(
            !has_start_drag(&cmds),
            "stationary press on whitespace without movement must not emit StartDrag: {cmds:?}"
        );
    }
}
