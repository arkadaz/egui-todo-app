use crate::app_data::TodoItem;
use chrono::NaiveDate;
use eframe::egui;
use std::collections::HashMap;

pub fn draw_todo_window(
    ctx: &egui::Context,
    is_open: &mut bool,
    todos_by_date: &mut HashMap<NaiveDate, Vec<TodoItem>>,
    new_todo_input: &mut String,
    selected_date: &mut NaiveDate,
) {
    let mut open = *is_open;
    egui::Window::new("To-Do List")
        .open(&mut open)
        .default_width(320.0)
        .default_height(500.0)
        .resizable(true)
        .show(ctx, |ui| {
            // --- Task Editor for Selected Day ---
            ui.vertical_centered(|ui| {
                ui.heading("Task Editor");
                ui.label(selected_date.format("%A, %B %-d, %Y").to_string());
            });
            ui.separator();

            let add_todo_response = ui
                .text_edit_singleline(new_todo_input)
                .on_hover_text("What needs to be done? (Press Enter to add)");
            if add_todo_response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !new_todo_input.trim().is_empty()
            {
                let todos_for_day = todos_by_date.entry(*selected_date).or_default();
                todos_for_day.push(TodoItem {
                    text: new_todo_input.trim().to_owned(),
                    completed: false,
                });
                new_todo_input.clear();
                add_todo_response.request_focus();
            }

            ui.add_space(5.0);

            let mut to_delete = None;
            let top_scroll_height = ui.available_height() * 0.4;

            // Scroll area for the current day's tasks
            egui::ScrollArea::vertical()
                .max_height(top_scroll_height)
                .show(ui, |ui| {
                    let todos_for_day = todos_by_date.entry(*selected_date).or_default();
                    if todos_for_day.is_empty() {
                        ui.label("No tasks for this day.");
                    } else {
                        for (i, todo) in todos_for_day.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut todo.completed, &todo.text);
                                if ui.button("❌").on_hover_text("Remove task").clicked() {
                                    to_delete = Some(i);
                                }
                            });
                        }
                    }
                });

            if let Some(index) = to_delete {
                todos_by_date
                    .entry(*selected_date)
                    .or_default()
                    .remove(index);
            }

            ui.separator();

            // --- Task History Section ---
            ui.heading("Task History");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut past_dates: Vec<_> = todos_by_date
                    .iter()
                    .filter(|(date, tasks)| !tasks.is_empty() && **date < *selected_date)
                    .map(|(date, _)| *date)
                    .collect();
                past_dates.sort_unstable_by(|a, b| b.cmp(a));

                if past_dates.is_empty() {
                    ui.label("No tasks from previous days.");
                } else {
                    for date in past_dates {
                        if let Some(tasks) = todos_by_date.get_mut(&date) {
                            if tasks.is_empty() {
                                continue;
                            }

                            ui.label(
                                egui::RichText::new(date.format("%A, %B %-d").to_string()).strong(),
                            );
                            ui.add_space(2.0);

                            // Iterate mutably and use a checkbox for each task
                            for task in tasks.iter_mut() {
                                ui.checkbox(&mut task.completed, &task.text);
                            }
                            ui.separator();
                        }
                    }
                }
            });
        });

    *is_open = open;
}
