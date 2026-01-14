use crate::domain::AppData;
use std::fs;
use std::path::PathBuf;

pub trait Persistence {
    fn load(&self) -> Result<AppData, Box<dyn std::error::Error>>;
    fn save(&self, data: &AppData) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct JsonFilePersistence {
    file_path: PathBuf,
}

impl JsonFilePersistence {
    pub fn new(filename: &str) -> Result<Self, std::io::Error> {
        let exe_path = std::env::current_exe()?;
        let dir = exe_path
            .parent()
            .unwrap_or(&PathBuf::from(""))
            .to_path_buf();
        Ok(Self {
            file_path: dir.join(filename),
        })
    }
}

impl Persistence for JsonFilePersistence {
    fn load(&self) -> Result<AppData, Box<dyn std::error::Error>> {
        if !self.file_path.exists() {
            return Ok(AppData::default());
        }
        let json_str = fs::read_to_string(&self.file_path)?;
        Ok(serde_json::from_str(&json_str)?)
    }

    fn save(&self, data: &AppData) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(&self.file_path, serde_json::to_string_pretty(data)?)?;
        Ok(())
    }
}
