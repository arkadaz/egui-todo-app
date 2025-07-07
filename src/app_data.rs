use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use chrono::NaiveDate;

const DATA_FILE: &str = "focushub_data.json";

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

fn get_data_path() -> Result<PathBuf, std::io::Error> {
    let exe_path = std::env::current_exe()?;
    let dir = exe_path.parent().unwrap_or(&PathBuf::from("")).to_path_buf();
    Ok(dir.join(DATA_FILE))
}

pub fn save(data: &AppData) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_data_path()?;
    fs::write(path, serde_json::to_string_pretty(data)?)?;
    Ok(())
}

pub fn load() -> Result<AppData, Box<dyn std::error::Error>> {
    let path = get_data_path()?;
    let json_str = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json_str)?)
}