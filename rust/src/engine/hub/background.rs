//! The background: animated or not, the chosen image, and the Home screen offer.

use super::*;

impl Hub {
    /// Whether the background GIF plays (turning it off saves battery).
    pub fn animate_background(&self) -> bool {
        self.data.settings.animate_background
    }

    pub fn set_animate_background(&mut self, animate: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        self.data.settings.animate_background = animate;
        self.save(now_ms, [Change::Settings])
    }

    /// Whether the app already offered to put its icon on the Home screen.
    pub fn home_icon_offered(&self) -> bool {
        self.data.settings.home_icon_offered
    }

    pub fn set_home_icon_offered(&mut self, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        self.data.settings.home_icon_offered = true;
        self.save(now_ms, [Change::Settings])
    }

    /// The custom background image, if one is set and still exists.
    pub fn background_path(&self) -> Option<String> {
        self.data
            .gif_path
            .as_ref()
            .filter(|path| Path::new(path).is_file())
            .cloned()
    }

    /// Stores a copy of the chosen image in the app's own folder, so it keeps working even
    /// if the original file is moved or deleted.
    pub fn set_background(&mut self, file_name: &str, bytes: &[u8], now_ms: i64) -> Result<String> {
        let extension = Path::new(file_name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !BACKGROUND_EXTENSIONS.contains(&extension.as_str()) {
            bail!("Please choose a GIF, PNG, JPG or WebP image.");
        }
        if bytes.is_empty() {
            bail!("That file is empty.");
        }
        self.reload_if_changed()?;
        let dir = self.data_dir.join(BACKGROUNDS_DIR);
        fs::create_dir_all(&dir)?;
        // A new name each time, so the UI never shows a cached old image.
        let path = dir.join(format!("background-{now_ms}.{extension}"));
        fs::write(&path, bytes).with_context(|| format!("Couldn't save {}", path.display()))?;
        self.remove_custom_background();
        self.data.gif_path = Some(path.display().to_string());
        self.save(now_ms, [Change::Settings])?;
        Ok(path.display().to_string())
    }

    pub fn clear_background(&mut self, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        self.remove_custom_background();
        self.data.gif_path = None;
        self.save(now_ms, [Change::Settings])
    }

    /// Deletes the current background, but only if it's our own copy.
    fn remove_custom_background(&self) {
        if let Some(old) = &self.data.gif_path {
            let old = Path::new(old);
            if old.starts_with(self.data_dir.join(BACKGROUNDS_DIR)) {
                let _ = fs::remove_file(old);
            }
        }
    }
}
