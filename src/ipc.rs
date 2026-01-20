use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// Socket path
pub fn socket_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("hypr-ringlight.sock")
}

/// Commands that can be sent via IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "value")]
pub enum Command {
    SetColor(String),
    SetThickness(u32),
    SetOpacity(f64),
    SetGlow(u32),
    SetCornerRadius(f64),
    SetAnimation(String),
    SetAnimationSpeed(u32),
    SetVisible(bool),
    GetState,
    Quit,
}

/// Response from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub color: String,
    pub thickness: u32,
    pub opacity: f64,
    pub glow: u32,
    pub corner_radius: f64,
    pub animation: String,
    pub animation_speed: u32,
    pub visible: bool,
}

/// Shared state that can be modified via IPC
pub struct IpcState {
    pub color_r: AtomicU8,
    pub color_g: AtomicU8,
    pub color_b: AtomicU8,
    pub thickness: AtomicU32,
    pub opacity: AtomicU32, // stored as opacity * 1000
    pub glow: AtomicU32,
    pub corner_radius: AtomicU32, // stored as radius * 1000
    pub animation_mode: AtomicU8,
    pub animation_speed: AtomicU32,
    pub visible: std::sync::atomic::AtomicBool,
}

impl IpcState {
    pub fn new(
        color: (u8, u8, u8),
        thickness: u32,
        opacity: f64,
        glow: u32,
        corner_radius: f64,
        animation: u8,
        animation_speed: u32,
    ) -> Self {
        Self {
            color_r: AtomicU8::new(color.0),
            color_g: AtomicU8::new(color.1),
            color_b: AtomicU8::new(color.2),
            thickness: AtomicU32::new(thickness),
            opacity: AtomicU32::new((opacity * 1000.0) as u32),
            glow: AtomicU32::new(glow),
            corner_radius: AtomicU32::new((corner_radius * 1000.0) as u32),
            animation_mode: AtomicU8::new(animation),
            animation_speed: AtomicU32::new(animation_speed),
            visible: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn get_color(&self) -> (u8, u8, u8) {
        (
            self.color_r.load(Ordering::Relaxed),
            self.color_g.load(Ordering::Relaxed),
            self.color_b.load(Ordering::Relaxed),
        )
    }

    pub fn set_color(&self, r: u8, g: u8, b: u8) {
        self.color_r.store(r, Ordering::Relaxed);
        self.color_g.store(g, Ordering::Relaxed);
        self.color_b.store(b, Ordering::Relaxed);
    }

    pub fn get_opacity(&self) -> f64 {
        self.opacity.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn set_opacity(&self, opacity: f64) {
        self.opacity.store((opacity * 1000.0) as u32, Ordering::Relaxed);
    }

    pub fn get_corner_radius(&self) -> f64 {
        self.corner_radius.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn set_corner_radius(&self, radius: f64) {
        self.corner_radius.store((radius * 1000.0) as u32, Ordering::Relaxed);
    }

    pub fn get_thickness(&self) -> u32 {
        self.thickness.load(Ordering::Relaxed)
    }

    pub fn get_glow(&self) -> u32 {
        self.glow.load(Ordering::Relaxed)
    }

    pub fn get_animation_mode(&self) -> u8 {
        self.animation_mode.load(Ordering::Relaxed)
    }

    pub fn get_animation_speed(&self) -> u32 {
        self.animation_speed.load(Ordering::Relaxed)
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }
}

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

fn animation_from_string(s: &str) -> u8 {
    match s.to_lowercase().as_str() {
        "pulse" => 1,
        "rainbow" => 2,
        "breathe" => 3,
        _ => 0,
    }
}

fn animation_to_string(mode: u8) -> String {
    match mode {
        1 => "pulse",
        2 => "rainbow",
        3 => "breathe",
        _ => "none",
    }.to_string()
}

fn color_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("{:02x}{:02x}{:02x}", r, g, b)
}

/// Handle a single client connection
fn handle_client(mut stream: UnixStream, state: &Arc<IpcState>) -> bool {
    let reader = BufReader::new(stream.try_clone().unwrap());
    
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        
        let cmd: Command = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        match cmd {
            Command::SetColor(hex) => {
                let (r, g, b) = parse_hex_color(&hex);
                state.set_color(r, g, b);
            }
            Command::SetThickness(v) => {
                state.thickness.store(v, Ordering::Relaxed);
            }
            Command::SetOpacity(v) => {
                state.set_opacity(v);
            }
            Command::SetGlow(v) => {
                state.glow.store(v, Ordering::Relaxed);
            }
            Command::SetCornerRadius(v) => {
                state.set_corner_radius(v);
            }
            Command::SetAnimation(s) => {
                state.animation_mode.store(animation_from_string(&s), Ordering::Relaxed);
            }
            Command::SetAnimationSpeed(v) => {
                state.animation_speed.store(v, Ordering::Relaxed);
            }
            Command::SetVisible(v) => {
                state.visible.store(v, Ordering::Relaxed);
            }
            Command::GetState => {
                let (r, g, b) = state.get_color();
                let response = State {
                    color: color_to_hex(r, g, b),
                    thickness: state.get_thickness(),
                    opacity: state.get_opacity(),
                    glow: state.get_glow(),
                    corner_radius: state.get_corner_radius(),
                    animation: animation_to_string(state.get_animation_mode()),
                    animation_speed: state.get_animation_speed(),
                    visible: state.is_visible(),
                };
                let json = serde_json::to_string(&response).unwrap();
                let _ = writeln!(stream, "{}", json);
            }
            Command::Quit => {
                return true; // Signal to quit
            }
        }
    }
    
    false
}

/// Start the IPC server in a background thread
pub fn start_server(state: Arc<IpcState>) {
    let path = socket_path();
    
    // Remove old socket if exists
    let _ = std::fs::remove_file(&path);
    
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to create IPC socket: {}", e);
            return;
        }
    };
    
    // Set socket permissions
    let _ = std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
    
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let state = state.clone();
                    std::thread::spawn(move || {
                        if handle_client(stream, &state) {
                            std::process::exit(0);
                        }
                    });
                }
                Err(_) => continue,
            }
        }
    });
}

/// Client: send a command to the running instance
pub fn send_command(cmd: &Command) -> Result<Option<State>, String> {
    let path = socket_path();
    
    let mut stream = UnixStream::connect(&path)
        .map_err(|_| "hypr-ringlight is not running".to_string())?;
    
    let json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
    writeln!(stream, "{}", json).map_err(|e| e.to_string())?;
    
    if matches!(cmd, Command::GetState) {
        let reader = BufReader::new(stream);
        if let Some(Ok(line)) = reader.lines().next() {
            let state: State = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            return Ok(Some(state));
        }
    }
    
    Ok(None)
}

/// Check if the server is running
pub fn is_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

impl IpcState {
    /// Save current state to config file
    pub fn save_to_config(&self) {
        use crate::config::Config;
        
        // Load existing config to preserve bar settings
        let existing = Config::load();
        
        let (r, g, b) = self.get_color();
        let config = Config {
            color: color_to_hex(r, g, b),
            thickness: self.get_thickness(),
            opacity: self.get_opacity(),
            glow: self.get_glow(),
            corner_radius: self.get_corner_radius(),
            animation: animation_to_string(self.get_animation_mode()),
            animation_speed: self.get_animation_speed(),
            bar_height: existing.bar_height,
            bar_position: existing.bar_position,
        };
        
        if let Err(e) = config.save() {
            eprintln!("Warning: Failed to save config: {}", e);
        }
    }
}
