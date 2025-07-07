pub mod calendar;
pub mod central_panel;
pub mod rewards;
pub mod stats;
pub mod todo_window;

pub use calendar::draw_calendar_window;
pub use central_panel::draw_central_panel;
pub use rewards::draw_rewards_window;
pub use stats::draw_stats_window;
pub use todo_window::draw_todo_window;
use eframe::egui;

pub fn draw_notification_window(ctx: &egui::Context, ui_manager: &mut crate::UIManager) {
    if ui_manager.show_notification {
        let mut is_open = true;
        egui::Window::new(&ui_manager.notification_title)
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(&ui_manager.notification_message);
                if ui.button("Dismiss").clicked() {
                    ui_manager.show_notification = false;
                }
            });
        if !is_open { ui_manager.show_notification = false; }
    }
}