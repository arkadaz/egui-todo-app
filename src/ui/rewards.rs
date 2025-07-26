use crate::app_data::Reward;
use eframe::egui;

pub fn draw_rewards_window(
    ctx: &egui::Context,
    is_open: &mut bool,
    rewards: &mut Vec<Reward>,
    new_reward_input: &mut String,
) {
    let mut open = *is_open;
    egui::Window::new("Rewards")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Add a New Reward");
            if ui
                .text_edit_singleline(new_reward_input)
                .on_hover_text("Enter a new reward...")
                .lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !new_reward_input.trim().is_empty()
            {
                rewards.push(Reward {
                    name: new_reward_input.trim().to_owned(),
                    completed: false,
                });
                new_reward_input.clear();
            }
            ui.separator();
            ui.heading("Your Rewards");
            rewards.sort_by_key(|r| r.completed);
            let mut to_delete = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, reward) in rewards.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut reward.completed, &reward.name);
                        if ui.button("❌").on_hover_text("Remove reward").clicked() {
                            to_delete = Some(i);
                        }
                    });
                }
            });
            if let Some(index) = to_delete {
                rewards.remove(index);
            }
        });
    *is_open = open;
}
