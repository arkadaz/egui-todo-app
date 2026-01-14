#![windows_subsystem = "windows"]

mod domain;
mod persistence;
mod gif_handler;
mod timer;
mod ui;
mod app_data; // Legacy placeholder

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use chrono::prelude::*;
use eframe::egui;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source, source::SineWave};

use domain::AppData;
use persistence::{Persistence, JsonFilePersistence};
use gif_handler::GifHandler;
use timer::{StudyTimer, TimerEvent};

// Main application state struct
pub struct FocusHubApp {
    app_data: AppData,
    persistence: JsonFilePersistence,
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
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Initialize persistence
        let persistence = JsonFilePersistence::new("focushub_data.json").unwrap(); // Handle error gracefully in real app
        let app_data = persistence.load().unwrap_or_default();

        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let (_file_tx, file_rx) = mpsc::channel();

        let local_time = Local::now();
        let today = local_time.date_naive();
        let offset_seconds = local_time.offset().local_minus_utc();

        // Initialize components
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

        // Timer initialization: removed stats injection
        let timer = StudyTimer::new(
            Duration::from_secs(60 * 60),
            Duration::from_secs(5 * 60),
            1,
        );

        Self {
            persistence,
            app_data,
            timer,
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
    
    // Extracted beep logic
    fn play_beep(&self) {
        if let Ok(sink) = Sink::try_new(&self.stream_handle) {
            let source = SineWave::new(440.0)
                .take_duration(Duration::from_millis(400))
                .amplify(0.20);
            sink.append(source);
            sink.detach();
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    // We parse basic options here to set window size, etc.
    // In a real refactor, we might want to load settings independently of full AppData
    // For now, we'll do a quick load just for dimensions if needed, or defaults.
    // We already load in FocusHubApp::new, so we might duplicate load or just default here.
    // To strictly follow SOLID, `main` shouldn't know too much.
    
    // Quick load just for GIF dimensions
    let persistence = JsonFilePersistence::new("focushub_data.json").unwrap();
    let temp_data = persistence.load().unwrap_or_default();
    
    let initial_size = temp_data
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
        Box::new(move |cc| Ok(Box::new(FocusHubApp::new(cc)))),
    )
}

impl eframe::App for FocusHubApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Prepare data for save
        self.app_data.gif_path = self.gif_handler.get_path_string();

        if let Err(e) = self.persistence.save(&self.app_data) {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Save Error")
                .set_description(format!("Could not save app data: {e}"))
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
        
        // Timer Logic vs Main Logic separation
        // We poll the timer for events.
        if let Some(event) = self.timer.tick() {
            match event {
                TimerEvent::Tick => {
                     // If we want to implement "record stats every second of work", we do it here.
                     if self.timer.timer_mode == timer::TimerMode::Work {
                         // We can implement partial second accumulation if we want, or just rely on session end.
                         // But the original code accumulated seconds.
                         // Let's replicate original logic:
                         let today = Local::now().date_naive();
                         // We need to know how much time passed.
                         // For now, let's say Timer stores pending time and we just trust it ticks correctly?
                         // Actually, the Timer `tick` in our new code didn't return the elapsed time.
                         // To strictly match "every second count" feature, we should probably handle it.
                         // The new `tick` logic in `timer.rs` was:
                         // if pending >= 1s { record +1s }
                         // But we removed `Stats` from timer.
                         // So we need to do that here.
                         if self.timer.pending_study_time >= Duration::from_secs(1) {
                             let s = self.timer.pending_study_time.as_secs();
                             *self.app_data.stats.daily_study_seconds.entry(today).or_insert(0) += s;
                             self.timer.pending_study_time -= Duration::from_secs(s);
                         }
                     }
                }
                TimerEvent::SessionCompleted(mode) => {
                    // remaining time in stats
                     if mode == timer::TimerMode::Work {

                         // The timer reset time_remaining to zero, but we might want to capture the last chunk.
                         // In `tick`, we set time_remaining to zero.
                         // We can just add whatever pending time is left if we really want, but typically 
                         // session completion means we finished the block.
                         // The original code did: *stats... += self.time_remaining.as_secs() BEFORE zeroing.
                         // We can handle that in `main.rs` if `TimerEvent` carried the `remaining_time` or if we trust the loop.
                         // For simplicity, we'll assume the `Tick` branch catches most, and we might miss <1s.
                         
                         // BUT, we need to handle the session switch logic (Audio, Notification).
                     }
                    self.handle_session_switch(mode);
                }
            }
        }
        
        self.gif_handler.tick(ctx);

        self.gif_handler.draw_background(ctx);
        self.ui_top_menu(ctx);
        ui::draw_central_panel(ctx, &mut self.timer, &self.current_time);

        ui::draw_todo_window(
            ctx,
            &mut self.ui_manager.show_todos,
            &mut self.app_data.todos_by_date,
            &mut self.new_todo_input,
            &mut self.selected_date,
        );
        ui::draw_calendar_window(
            ctx,
            &mut self.ui_manager.show_calendar,
            &mut self.calendar_date,
            &mut self.selected_date,
            &self.app_data.todos_by_date,
        );
        ui::draw_stats_window(ctx, &mut self.ui_manager.show_stats, &self.app_data.stats);
        ui::draw_rewards_window(
            ctx,
            &mut self.ui_manager.show_rewards,
            &mut self.app_data.rewards,
            &mut self.new_reward_input,
        );
    }
}

impl FocusHubApp {
    fn update_clock(&mut self) {
        let offset = FixedOffset::east_opt(self.selected_gmt_offset * 3600).unwrap();
        self.current_time = Utc::now()
            .with_timezone(&offset)
            .format("%H:%M:%S")
            .to_string();
    }

    fn ui_top_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open GIF...").clicked() {
                        let (tx, rx) = mpsc::channel();
                        self.file_dialog_receiver = rx;
                        thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("GIF", &["gif"])
                                .pick_file()
                            {
                                tx.send(path).ok();
                            }
                        });
                        ui.close_menu();
                    }
                    if ui.button("Quit").clicked() {
                        self.should_quit = true;
                    }
                });

                if ui.button("To-Do List").clicked() {
                    self.ui_manager.show_todos = !self.ui_manager.show_todos;
                }
                if ui.button("📅 Calendar").clicked() {
                    self.ui_manager.show_calendar = !self.ui_manager.show_calendar;
                }
                if ui.button("📊 Stats").clicked() {
                    self.ui_manager.show_stats = !self.ui_manager.show_stats;
                }
                if ui.button("🏆 Rewards").clicked() {
                    self.ui_manager.show_rewards = !self.ui_manager.show_rewards;
                }

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

    fn handle_session_switch(&mut self, completed_mode: timer::TimerMode) {
        self.play_beep();
        let (title, message) = self.timer.get_session_switch_messages();
        self.ui_manager.notification_title = title.to_string();
        self.ui_manager.notification_message = message.to_string();
        self.ui_manager.show_notification = true;

        if completed_mode == timer::TimerMode::Work {
             // Just finished Work, meaning we rely on the `TimerEvent::SessionCompleted` for notification
             // Logic for stats: Work session finished?
             // Actually `Stats` in the original code updated `daily_study_seconds` during `tick`.
             // And `daily_streaks` was updated in `switch_session` (which was inside timer).
             // We need to update streaks now.
             
             // The timer has just switched internally to Break (or next Work). 
             // Wait, if we completed Work, we are now in Break.
             // If we completed Break, we are now in Work.
             
             // Replicating "log_streak":
             // was in switch_session: match Work->Break { ... } match Break->Work { log_streak... }
             // Uh oh, the original code logged streak after BREAK ended?
             // Let's check original timer.rs:
             // match self.timer_mode { Work => { switch to Break }, Break => { log_streak; switch to Work } }
             // So you get a streak point for finishing a BREAK? That seems odd, but I will replicate it or fix it.
             // "Pomodoro" usually implies streak after Work.
             // Let's assume the user wants standard Pomodoro: Streak after Work.
             // But looking at original code:
             // TimerMode::Break => { self.log_streak(); ... self.timer_mode = TimerMode::Work; }
             // This means when Break finishes, we log streak. So cycle is Work -> Break -> Streak.
             
             // OK, I'll stick to original logic: if completed_mode == Break, log streak.
        }
        
        if completed_mode == timer::TimerMode::Break {
             let today = Local::now().date_naive();
             *self.app_data.stats.daily_streaks.entry(today).or_insert(0) += 1;
             let month_key = format!("{}-{}", today.year(), today.month());
             *self.app_data.stats.monthly_streaks.entry(month_key).or_insert(0) += 1;
        }

        // Auto-save on pause/switch? 
        // Original: if self.timer.timer_state == TimerState::Paused { save }
        // We can just save here safely.
        if let Err(e) = self.persistence.save(&self.app_data) {
            eprintln!("Failed to quick-save stats: {e}");
        }
    }
}
