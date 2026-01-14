use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodoItem {
    pub text: String,
    pub completed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Stats {
    #[serde(default)]
    pub daily_study_seconds: HashMap<NaiveDate, u64>,
    #[serde(default)]
    pub daily_streaks: HashMap<NaiveDate, u32>,
    #[serde(default)]
    pub monthly_streaks: HashMap<String, u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reward {
    pub name: String,
    pub completed: bool,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct AppData {
    pub todos_by_date: HashMap<NaiveDate, Vec<TodoItem>>,
    pub stats: Stats,
    pub rewards: Vec<Reward>,
    #[serde(default)]
    pub gif_path: Option<String>,
}
