use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Ring light configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Ring color in hex format (e.g., ff0000 for red)
    #[serde(default = "default_color")]
    pub color: String,

    /// Ring thickness in pixels
    #[serde(default = "default_thickness")]
    pub thickness: u32,

    /// Ring opacity (0.0 - 1.0)
    #[serde(default = "default_opacity")]
    pub opacity: f64,

    /// Blur/glow radius (softness)
    #[serde(default = "default_glow")]
    pub glow: u32,

    /// Corner radius multiplier (relative to thickness)
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f64,

    /// Animation mode: none, pulse, rainbow, breathe
    #[serde(default = "default_animation")]
    pub animation: String,

    /// Animation speed (frames per cycle, lower = faster)
    #[serde(default = "default_animation_speed")]
    pub animation_speed: u32,

    /// Waybar/bar height in pixels
    #[serde(default = "default_bar_height")]
    pub bar_height: u32,

    /// Waybar/bar position: top, bottom, left, right
    #[serde(default = "default_bar_position")]
    pub bar_position: String,
}

fn default_color() -> String { "ffffff".to_string() }
fn default_thickness() -> u32 { 80 }
fn default_opacity() -> f64 { 1.0 }
fn default_glow() -> u32 { 80 }
fn default_corner_radius() -> f64 { 2.5 }
fn default_animation() -> String { "none".to_string() }
fn default_animation_speed() -> u32 { 120 }
fn default_bar_height() -> u32 { 35 }
fn default_bar_position() -> String { "top".to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            color: default_color(),
            thickness: default_thickness(),
            opacity: default_opacity(),
            glow: default_glow(),
            corner_radius: default_corner_radius(),
            animation: default_animation(),
            animation_speed: default_animation_speed(),
            bar_height: default_bar_height(),
            bar_position: default_bar_position(),
        }
    }
}

impl Config {
    /// Get the config file path
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hypr-ringlight")
            .join("config.toml")
    }

    /// Load config from file, or return default if not found
    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match toml::from_str(&content) {
                        Ok(config) => return config,
                        Err(e) => eprintln!("Warning: Failed to parse config: {}", e),
                    }
                }
                Err(e) => eprintln!("Warning: Failed to read config: {}", e),
            }
        }
        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        
        Ok(())
    }

    /// Parse animation string to u8
    pub fn animation_mode(&self) -> u8 {
        match self.animation.to_lowercase().as_str() {
            "pulse" => 1,
            "rainbow" => 2,
            "breathe" => 3,
            _ => 0, // none
        }
    }

    /// Parse bar position string
    pub fn bar_position_enum(&self) -> BarPosition {
        match self.bar_position.to_lowercase().as_str() {
            "bottom" => BarPosition::Bottom,
            "left" => BarPosition::Left,
            "right" => BarPosition::Right,
            _ => BarPosition::Top,
        }
    }
}

/// Waybar position
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}
