use crate::app_data::TodoItem;
use chrono::{Datelike, Local, Month, NaiveDate};
use eframe::egui;
use num_traits::FromPrimitive;
use std::collections::HashMap;

pub fn draw_calendar_window(
    ctx: &egui::Context,
    is_open: &mut bool,
    calendar_date: &mut NaiveDate,
    selected_date: &mut NaiveDate,
    todos_by_date: &HashMap<NaiveDate, Vec<TodoItem>>,
) {
    let mut open = *is_open;
    let month_str = Month::from_u32(calendar_date.month()).unwrap().name();
    let window_title = format!("{} {}", month_str, calendar_date.year());

    egui::Window::new(window_title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("◀").clicked() {
                    let (year, month) = if calendar_date.month() == 1 {
                        (calendar_date.year() - 1, 12)
                    } else {
                        (calendar_date.year(), calendar_date.month() - 1)
                    };
                    *calendar_date = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
                }
                if ui.button("Today").clicked() {
                    let today = Local::now().date_naive();
                    *calendar_date = today;
                    *selected_date = today;
                }
                if ui.button("▶").clicked() {
                    let (year, month) = if calendar_date.month() == 12 {
                        (calendar_date.year() + 1, 1)
                    } else {
                        (calendar_date.year(), calendar_date.month() + 1)
                    };
                    *calendar_date = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
                }
            });
            ui.separator();

            egui::Grid::new("calendar_grid")
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for day in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
                        ui.label(egui::RichText::new(day).strong());
                    }
                    ui.end_row();

                    let first_day = calendar_date.with_day(1).unwrap();
                    let weekday_offset = first_day.weekday().num_days_from_monday();
                    for _ in 0..weekday_offset {
                        ui.label("");
                    }

                    let days_in_month =
                        NaiveDate::from_ymd_opt(calendar_date.year(), calendar_date.month() + 1, 1)
                            .unwrap_or_else(|| {
                                NaiveDate::from_ymd_opt(calendar_date.year() + 1, 1, 1).unwrap()
                            })
                            .signed_duration_since(first_day)
                            .num_days() as u32;

                    for day in 1..=days_in_month {
                        let current_cell_date = calendar_date.with_day(day).unwrap();
                        let is_today = current_cell_date == Local::now().date_naive();
                        let is_selected = current_cell_date == *selected_date;
                        let has_todos = todos_by_date
                            .get(&current_cell_date)
                            .is_some_and(|v| !v.is_empty());

                        let mut frame = egui::Frame::central_panel(ui.style())
                            .inner_margin(egui::Margin::same(4.0));
                        if is_today {
                            frame = frame.fill(ui.visuals().selection.bg_fill);
                        }
                        if is_selected {
                            frame = frame.stroke(ui.visuals().widgets.active.bg_stroke);
                        }

                        frame.show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                let mut label = egui::RichText::new(day.to_string());
                                if is_today {
                                    label =
                                        label.color(ui.visuals().selection.stroke.color).strong();
                                }
                                let response =
                                    ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                                if response.clicked() {
                                    *selected_date = current_cell_date;
                                }
                                if has_todos {
                                    let dot_pos =
                                        response.rect.center_bottom() + egui::vec2(0.0, -2.0);
                                    ui.painter().circle_filled(
                                        dot_pos,
                                        2.0,
                                        ui.visuals().text_color(),
                                    );
                                }
                            });
                        });
                        if (day + weekday_offset) % 7 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
    *is_open = open;
}
