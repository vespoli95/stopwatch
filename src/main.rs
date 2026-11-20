#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native_windows;
mod stopwatch;
mod updater;

use std::sync::mpsc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2,
    ViewportCommand, WindowLevel,
};

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
const COMPACT_SIZE: f32 = 200.0;
const TIMER_REFRESH_INTERVAL: Duration = Duration::from_millis(33);
const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_millis(250);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([560.0, 470.0])
            .with_min_inner_size([400.0, 300.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|creation_context| {
            StopwatchApp::apply_theme(&creation_context.egui_ctx);
            Ok(Box::new(StopwatchApp::default()))
        }),
    )
}

struct StopwatchApp {
    stopwatch: Stopwatch,
    pinned: bool,
    release: Option<Release>,
    update_state: UpdateState,
    update_rx: Option<mpsc::Receiver<Result<Option<Release>, String>>>,
    install_rx: Option<mpsc::Receiver<Result<(), InstallError>>>,
    launch_update_checked: bool,
    show_laps: bool,
    pending_pin_position_frames: u8,
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
        }
    }
}

impl StopwatchApp {
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
            (560.0, 470.0)
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

    fn check_for_updates(&mut self) {
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
        std::thread::spawn(move || {
            let _ = tx.send(updater::fetch_latest_release(&repository));
        });
    }

    fn handle_background_work(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update_rx
            && let Ok(result) = rx.try_recv()
        {
            self.update_rx = None;
            match result {
                Ok(Some(release)) if release.tag_name != CURRENT_RELEASE => {
                    self.release = Some(release);
                    self.update_state = UpdateState::Available;
                }
                Ok(_) => self.update_state = UpdateState::Current,
                Err(message) => self.update_state = UpdateState::Failed(message),
            }
        }

        if let Some(rx) = &self.install_rx
            && let Ok(result) = rx.try_recv()
        {
            self.install_rx = None;
            match result {
                Ok(()) => ctx.send_viewport_cmd(ViewportCommand::Close),
                Err(error) => self.update_state = UpdateState::Failed(error.to_string()),
            }
        }

        if !self.launch_update_checked {
            self.launch_update_checked = true;
            self.check_for_updates();
        }
    }

    fn install_update(&mut self) {
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
        std::thread::spawn(move || {
            let _ = tx.send(updater::download_and_launch_installer(&installer_url));
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let compact = self.pinned;
        let height = if compact { 38.0 } else { 62.0 };
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
                        self.install_update();
                    }
                });
            }
            UpdateState::Installing => {
                ui.label(RichText::new("Downloading update…").small());
            }
            UpdateState::Failed(message) => {
                ui.colored_label(TEXT, message);
                if ui.button("Retry update check").clicked() {
                    self.check_for_updates();
                }
            }
            UpdateState::Current => {
                ui.label(RichText::new("Stopwatch is up to date.").small());
            }
            UpdateState::Idle | UpdateState::Unavailable => {}
        }
    }
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
            ctx.request_repaint_after(TIMER_REFRESH_INTERVAL);
        }
        if self.update_rx.is_some() || self.install_rx.is_some() {
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
                    });
                });

                ui.add_space(if self.pinned { 5.0 } else { 28.0 });

                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.stopwatch.formatted())
                            .monospace()
                            .color(TEXT)
                            .size(if self.pinned { 23.0 } else { 64.0 }),
                    );
                    ui.add_space(if self.pinned { 8.0 } else { 24.0 });
                    self.controls(ui);
                });

                if !self.pinned {
                    if !self.stopwatch.laps().is_empty() {
                        ui.add_space(22.0);
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
                                        .max_height(130.0)
                                        .show(ui, |ui| {
                                            for (index, lap) in
                                                self.stopwatch.laps().iter().enumerate().rev()
                                            {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(format!("Lap {}", index + 1))
                                                            .color(MUTED_TEXT),
                                                    );
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            ui.monospace(
                                                                RichText::new(
                                                                    Stopwatch::format_duration(
                                                                        *lap,
                                                                    ),
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
                }
            });
    }
}
