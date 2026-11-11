use serde::{Deserialize, Serialize};
use std::{cell::Cell, path::Path};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub vertical: bool,
    pub wrap: bool,
    pub design: u8,
    pub accent: u8,
    pub font_size: i32,
    pub update_check: bool,
    pub update_install: bool,
    pub image_auto_size: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            vertical: false,
            wrap: false,
            design: 0,
            accent: 0,
            font_size: 16,
            update_check: true,
            update_install: false,
            image_auto_size: true,
        }
    }
}
thread_local! { static CURRENT: Cell<Preferences> = Cell::new(Preferences::default()); }
pub fn get() -> Preferences {
    CURRENT.with(Cell::get)
}
pub fn set(value: Preferences) {
    CURRENT.with(|p| p.set(value));
}
pub fn load(path: &Path) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut value = serde_json::from_str::<Preferences>(&text).unwrap_or_else(|_| Preferences {
        vertical: text.trim() == "vertical",
        ..Preferences::default()
    });
    value.font_size = value.font_size.clamp(12, 28);
    value.design = value.design.min(2);
    value.accent = value.accent.min(2);
    set(value);
}
pub fn save(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&get())?)
}
