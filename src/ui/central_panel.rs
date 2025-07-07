use std::time::Duration;
use eframe::egui;
use crate::timer::{StudyTimer, TimerMode, TimerState};

pub fn draw_central_panel(ctx: &egui::Context, timer: &mut StudyTimer, current_time: &str) {
    let panel_frame = egui::Frame {
        inner_margin: egui::Margin::same(10.0),
        fill: egui::Color32::from_rgba_unmultiplied(20, 20, 20, 180),
        rounding: egui::Rounding::same(10.0),
        ..Default::default()
    };

    egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            // Time and Timer heading moved here to match original layout
            ui.label(egui::RichText::new(current_time).size(24.0));
            ui.add_space(10.0);
            ui.heading("Pomodoro Timer");

            let timer_text = match timer.timer_mode {
                TimerMode::Work => "Study Time",
                TimerMode::Break => "Break Time",
            };
            ui.label(format!("{} ({}/{})", timer_text, timer.current_loop, timer.total_loops));

            let mins = timer.time_remaining.as_secs() / 60;
            let secs = timer.time_remaining.as_secs() % 60;
            ui.label(egui::RichText::new(format!("{:02}:{:02}", mins, secs)).size(60.0));

            let total_duration = match timer.timer_mode {
                TimerMode::Work => timer.work_duration,
                TimerMode::Break => timer.break_duration,
            };
            if total_duration.as_secs() > 0 {
                let progress = 1.0 - (timer.time_remaining.as_secs_f32() / total_duration.as_secs_f32());
                ui.add(egui::ProgressBar::new(progress).show_percentage());
            }

            ui.horizontal(|ui| {
                let button_text = if timer.timer_state == TimerState::Running { "Pause" } else { "Start" };
                if ui.button(button_text).clicked() {
                    timer.toggle_state();
                }
                if ui.button("Reset").clicked() {
                    timer.reset();
                }
            });
            ui.add_space(20.0);
        });

        ui.collapsing("Timer Settings", |ui| {
            let mut work_mins = timer.work_duration.as_secs() / 60;
            let mut work_secs = timer.work_duration.as_secs() % 60;
            let mut break_mins = timer.break_duration.as_secs() / 60;
            let mut break_secs = timer.break_duration.as_secs() % 60;
            let mut total_loops = timer.total_loops;

            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Study Time:");
                if ui.add(egui::DragValue::new(&mut work_mins).suffix("m").range(0..=120)).changed() { changed = true; }
                if ui.add(egui::DragValue::new(&mut work_secs).suffix("s").range(0..=59)).changed() { changed = true; }
            });
            ui.horizontal(|ui| {
                ui.label("Break Time:");
                if ui.add(egui::DragValue::new(&mut break_mins).suffix("m").range(0..=60)).changed() { changed = true; }
                if ui.add(egui::DragValue::new(&mut break_secs).suffix("s").range(0..=59)).changed() { changed = true; }
            });
            ui.horizontal(|ui| {
                ui.label("Number of Loops:");
                if ui.add(egui::DragValue::new(&mut total_loops).range(1..=20)).changed() { changed = true; }
            });

            if changed {
                timer.set_durations(
                    Duration::from_secs(work_mins * 60 + work_secs),
                    Duration::from_secs(break_mins * 60 + break_secs),
                    total_loops,
                );
            }
        });
    });
}