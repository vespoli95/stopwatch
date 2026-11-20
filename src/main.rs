#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod stopwatch;
mod updater;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, RichText, Vec2, ViewportCommand, WindowLevel};

use stopwatch::Stopwatch;
use updater::{InstallError, Release, UpdateState};

const APP_NAME: &str = "Stopwatch";
const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const CURRENT_RELEASE: &str = match option_env!("STOPWATCH_RELEASE_TAG") {
    Some(tag) => tag,
    None => "development",
};
const UPDATE_REPOSITORY: Option<&str> = option_env!("STOPWATCH_UPDATE_REPOSITORY");

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([520.0, 430.0])
            .with_min_inner_size([340.0, 240.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|_creation_context| Ok(Box::new(StopwatchApp::default()))),
    )
}

struct StopwatchApp {
    stopwatch: Stopwatch,
    pinned: bool,
    release: Option<Release>,
    update_state: UpdateState,
    update_rx: Option<mpsc::Receiver<Result<Release, String>>>,
    install_rx: Option<mpsc::Receiver<Result<(), InstallError>>>,
    last_update_check: Option<Instant>,
    show_laps: bool,
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
            last_update_check: None,
            show_laps: true,
        }
    }
}

impl StopwatchApp {
    fn toggle_pinned(&mut self, ctx: &egui::Context) {
        self.pinned = !self.pinned;
        let level = if self.pinned {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        let (width, height) = if self.pinned {
            (370.0, 170.0)
        } else {
            (520.0, 430.0)
        };
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(width, height)));

        if self.pinned {
            let monitor_size = ctx.input(|input| input.viewport().monitor_size);
            if let Some(monitor_size) = monitor_size {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
                    monitor_size.x - width - 16.0,
                    16.0,
                )));
            }
        }
    }

    fn check_for_updates(&mut self) {
        if self.update_rx.is_some() {
            return;
        }

        let Some(repository) = UPDATE_REPOSITORY else {
            self.update_state = UpdateState::Unavailable;
            return;
        };

        self.update_state = UpdateState::Checking;
        self.last_update_check = Some(Instant::now());
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
                Ok(release) if release.tag_name != CURRENT_RELEASE => {
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

        if self.last_update_check.is_none()
            || self
                .last_update_check
                .is_some_and(|last| last.elapsed() >= UPDATE_INTERVAL)
        {
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
        ui.horizontal(|ui| {
            let start_label = if self.stopwatch.is_running() {
                "Pause"
            } else if self.stopwatch.elapsed().is_zero() {
                "Start"
            } else {
                "Resume"
            };
            if ui.button(RichText::new(start_label).strong()).clicked() {
                self.stopwatch.toggle();
            }

            if ui
                .add_enabled(self.stopwatch.is_running(), egui::Button::new("Lap"))
                .clicked()
            {
                self.stopwatch.record_lap();
            }

            if ui
                .add_enabled(
                    !self.stopwatch.elapsed().is_zero(),
                    egui::Button::new("Reset"),
                )
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
                    ui.colored_label(
                        ui.visuals().selection.stroke.color,
                        "A Stopwatch update is available.",
                    );
                    if ui.button("Install update").clicked() {
                        self.install_update();
                    }
                });
            }
            UpdateState::Installing => {
                ui.label(RichText::new("Downloading update…").small());
            }
            UpdateState::Failed(message) => {
                ui.colored_label(egui::Color32::from_rgb(220, 90, 90), message);
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

impl eframe::App for StopwatchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_background_work(ctx);
        if self.stopwatch.is_running() {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.pinned {
                    ui.heading(APP_NAME);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let pin_label = if self.pinned {
                        "Unpin"
                    } else {
                        "Always on top"
                    };
                    if ui.button(pin_label).clicked() {
                        self.toggle_pinned(ctx);
                    }
                });
            });

            if !self.pinned {
                ui.add_space(20.0);
            }

            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new(self.stopwatch.formatted())
                        .monospace()
                        .size(if self.pinned { 40.0 } else { 58.0 }),
                );
                ui.add_space(8.0);
                self.controls(ui);
            });

            if !self.pinned {
                if !self.stopwatch.laps().is_empty() {
                    ui.horizontal(|ui| {
                        ui.heading("Laps");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.toggle_value(&mut self.show_laps, "Show");
                        });
                    });
                    if self.show_laps {
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .show(ui, |ui| {
                                for (index, lap) in self.stopwatch.laps().iter().enumerate().rev() {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Lap {}", index + 1));
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                ui.monospace(Stopwatch::format_duration(*lap));
                                            },
                                        );
                                    });
                                }
                            });
                    }
                }
                self.update_panel(ui);
            }
        });
    }
}
