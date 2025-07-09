use eframe::egui;
use chrono::{Datelike, Local};
use crate::app_data::Stats;

pub fn draw_stats_window(ctx: &egui::Context, is_open: &mut bool, stats: &Stats) {
    let mut open = *is_open;
    egui::Window::new("Your Stats")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("📊 Lifetime Summary");
            let total_seconds: u64 = stats.daily_study_seconds.values().sum();
            let days = total_seconds / (24 * 3600);
            let hours = (total_seconds % (24 * 3600)) / 3600;
            let minutes = (total_seconds % 3600) / 60;
            let seconds = total_seconds % 60;
            ui.label(format!("Total Study Time: {}d {}h {}m {}s", days, hours, minutes, seconds));
            ui.separator();

            let today = Local::now().date_naive();
            let today_sessions = stats.daily_streaks.get(&today).cloned().unwrap_or(0);
            let today_seconds = stats.daily_study_seconds.get(&today).cloned().unwrap_or(0);
            let today_h = today_seconds / 3600;
            let today_m = (today_seconds % 3600) / 60;
            let today_s = today_seconds % 60;

            let month_key = format!("{}-{}", today.year(), today.month());
            let this_month_sessions = stats.monthly_streaks.get(&month_key).cloned().unwrap_or(0);

            ui.heading("📅 Today's Progress");
            ui.label(format!("- Sessions Completed: {}", today_sessions));
            ui.label(format!("- Time Studied: {:02}:{:02}:{:02}", today_h, today_m, today_s));
            ui.separator();

            ui.heading("📅 This Month's Progress");
            ui.label(format!("- Sessions Completed: {}", this_month_sessions));
        });
    *is_open = open;
}