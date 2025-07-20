#![windows_subsystem = "windows"]

mod app_data;
mod gif_handler;
mod timer;
mod ui;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use chrono::prelude::*;
use eframe::egui;
use rodio::{OutputStream, OutputStreamHandle};

use app_data::AppData;
use gif_handler::GifHandler;
use timer::{StudyTimer, TimerState};

// Main application state struct
pub struct FocusHubApp {
    app_data: AppData,
    timer: StudyTimer,
    gif_handler: GifHandler,
    ui_manager: UIManager,

    // UI state and inputs
    new_todo_input: String,
    new_reward_input: String,
    selected_date: NaiveDate,
    calendar_date: NaiveDate,
    selected_gmt_offset: i32,
    repaint_fps: u64,
    current_time: String,
    should_quit: bool,

    // Asynchronous operations
    file_dialog_receiver: Receiver<PathBuf>,

    // Audio
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
}

// Manages the visibility of different UI windows
pub struct UIManager {
    show_todos: bool,
    show_calendar: bool,
    show_stats: bool,
    show_rewards: bool,
    show_notification: bool,
    notification_title: String,
    notification_message: String,
}

impl FocusHubApp {
    fn new(cc: &eframe::CreationContext<'_>, app_data: AppData) -> Self {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let (_file_tx, file_rx) = mpsc::channel();

        let local_time = Local::now();
        let today = local_time.date_naive();
        let offset_seconds = local_time.offset().local_minus_utc();

        let gif_path = app_data.gif_path.clone();
        let mut gif_handler = GifHandler::new();
        if let Some(path_str) = gif_path {
            if !gif_handler.load_from_path(PathBuf::from(path_str)) {
                 gif_handler.load_from_path(PathBuf::from("assets/background.gif"));
            }
        } else {
            gif_handler.load_from_path(PathBuf::from("assets/background.gif"));
        }
        gif_handler.prime_cache(&cc.egui_ctx);


        Self {
            timer: StudyTimer::new(
                app_data.stats.clone(),
                Duration::from_secs(1 * 60),
                Duration::from_secs(5 * 60),
                1,
            ),
            app_data,
            gif_handler,
            ui_manager: UIManager {
                show_todos: false,
                show_calendar: false,
                show_stats: false,
                show_rewards: false,
                show_notification: false,
                notification_title: String::new(),
                notification_message: String::new(),
            },
            new_todo_input: String::new(),
            new_reward_input: String::new(),
            selected_date: today,
            calendar_date: today,
            selected_gmt_offset: (offset_seconds / 3600),
            repaint_fps: 30,
            current_time: String::new(),
            should_quit: false,
            file_dialog_receiver: file_rx,
            _stream: stream,
            stream_handle,
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let app_data = app_data::load().unwrap_or_default();
    let initial_size = app_data
        .gif_path
        .as_ref()
        .and_then(|p| gif_handler::get_gif_dimensions(&PathBuf::from(p)).ok())
        .map(|(w, h)| egui::vec2(w as f32, h as f32))
        .unwrap_or_else(|| egui::vec2(500.0, 450.0));

    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon = eframe::icon_data::from_png_bytes(icon_bytes).ok();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_size)
            .with_icon(icon.unwrap()),
        ..Default::default()
    };

    eframe::run_native(
        "Focus Hub",
        options,
        Box::new(move |cc| Ok(Box::new(FocusHubApp::new(cc, app_data)))),
    )
}

impl eframe::App for FocusHubApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app_data.stats = self.timer.stats.clone();
        self.app_data.gif_path = self.gif_handler.get_path_string();

        if let Err(e) = app_data::save(&self.app_data) {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Save Error")
                .set_description(format!("Could not save app data: {}", e))
                .show();
        }
    }

    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(1000 / self.repaint_fps));

        self.update_clock();
        self.handle_file_dialog(ctx);
        if self.timer.tick() {
            self.handle_session_switch();
        }
        self.gif_handler.tick(ctx);

        self.gif_handler.draw_background(ctx);
        self.ui_top_menu(ctx);
        ui::draw_central_panel(ctx, &mut self.timer, &self.current_time);

        ui::draw_todo_window(ctx, &mut self.ui_manager.show_todos, &mut self.app_data.todos_by_date, &mut self.new_todo_input, &mut self.selected_date);
        ui::draw_calendar_window(ctx, &mut self.ui_manager.show_calendar, &mut self.calendar_date, &mut self.selected_date, &self.app_data.todos_by_date);
        ui::draw_stats_window(ctx, &mut self.ui_manager.show_stats, &self.timer.stats);
        ui::draw_rewards_window(ctx, &mut self.ui_manager.show_rewards, &mut self.app_data.rewards, &mut self.new_reward_input);
    }
}

impl FocusHubApp {
    fn update_clock(&mut self) {
        let offset = FixedOffset::east_opt(self.selected_gmt_offset * 3600).unwrap();
        self.current_time = Utc::now().with_timezone(&offset).format("%H:%M:%S").to_string();
    }

    fn ui_top_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open GIF...").clicked() {
                        let (tx, rx) = mpsc::channel();
                        self.file_dialog_receiver = rx;
                        thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new().add_filter("GIF", &["gif"]).pick_file() {
                                tx.send(path).ok();
                            }
                        });
                        ui.close_menu();
                    }
                    if ui.button("Quit").clicked() { self.should_quit = true; }
                });

                if ui.button("To-Do List").clicked() { self.ui_manager.show_todos = !self.ui_manager.show_todos; }
                if ui.button("📅 Calendar").clicked() { self.ui_manager.show_calendar = !self.ui_manager.show_calendar; }
                if ui.button("📊 Stats").clicked() { self.ui_manager.show_stats = !self.ui_manager.show_stats; }
                if ui.button("🏆 Rewards").clicked() { self.ui_manager.show_rewards = !self.ui_manager.show_rewards; }

                ui.menu_button("Settings", |ui| {
                    ui.label("Time Zone (GMT):");
                    ui.add(egui::DragValue::new(&mut self.selected_gmt_offset).range(-12..=14));
                    ui.separator();
                    ui.label("Max FPS:");
                    ui.add(egui::DragValue::new(&mut self.repaint_fps).range(5..=500));
                });
            });
        });
    }

    fn handle_file_dialog(&mut self, ctx: &egui::Context) {
        if let Ok(path) = self.file_dialog_receiver.try_recv() {
            if self.gif_handler.load_from_path(path.clone()) {
                self.gif_handler.prime_cache(ctx);
                self.app_data.gif_path = Some(path.to_string_lossy().into_owned());
            } else {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("GIF Load Error")
                    .set_description("Could not load the selected GIF.")
                    .show();
            }
        }
    }

    fn handle_session_switch(&mut self) {
        timer::play_beep(&self.stream_handle);
        let (title, message) = self.timer.get_session_switch_messages();
        self.ui_manager.notification_title = title.to_string();
        self.ui_manager.notification_message = message.to_string();
        self.ui_manager.show_notification = true;

        if self.timer.timer_state == TimerState::Paused {
            self.app_data.stats = self.timer.stats.clone();
            if let Err(e) = app_data::save(&self.app_data) {
                 eprintln!("Failed to quick-save stats: {}", e);
            }
        }
    }
}