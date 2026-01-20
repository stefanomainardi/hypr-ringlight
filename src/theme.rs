//! Omarchy theme integration
//!
//! Reads the current Omarchy theme colors and applies them to the ring light.
//! Listens for SIGUSR2 to reload theme colors (like other Omarchy apps).

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Omarchy theme colors (subset of what's in colors.toml)
#[derive(Debug, Deserialize)]
pub struct OmarchyColors {
    /// Accent color (used as ring light color)
    pub accent: Option<String>,
    /// Background color
    pub background: Option<String>,
    /// Foreground color
    pub foreground: Option<String>,
}

/// Get the path to current Omarchy theme colors
fn omarchy_colors_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarchy")
        .join("current")
        .join("theme")
        .join("colors.toml")
}

/// Check if Omarchy is installed (by checking if the theme path exists)
pub fn is_omarchy_installed() -> bool {
    omarchy_colors_path().parent().map(|p| p.exists()).unwrap_or(false)
}

/// Load Omarchy theme colors
pub fn load_omarchy_colors() -> Option<OmarchyColors> {
    let path = omarchy_colors_path();
    
    if !path.exists() {
        return None;
    }
    
    let content = fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

/// Get the accent color from Omarchy theme as RGB tuple
pub fn get_accent_color() -> Option<(u8, u8, u8)> {
    let colors = load_omarchy_colors()?;
    let accent = colors.accent?;
    Some(parse_hex_color(&accent))
}

/// Parse hex color string to RGB tuple
fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return (255, 255, 255);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    (r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#89b4fa"), (137, 180, 250));
        assert_eq!(parse_hex_color("89b4fa"), (137, 180, 250));
        assert_eq!(parse_hex_color("#ff0000"), (255, 0, 0));
    }
}
