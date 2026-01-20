use std::io::stdout;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::*,
};
use crate::config::Config;
use crate::ipc::{self, Command, MonitorState};
use crate::theme;

/// UI color theme - loaded from Omarchy if available, otherwise Catppuccin Mocha defaults
struct UiTheme {
    accent: Color,      // Primary accent color (mauve/highlight)
    secondary: Color,   // Secondary accent (blue)
    background: Color,  // Surface background
    text: Color,        // Normal text
    success: Color,     // Green/success
    warning: Color,     // Yellow/warning
}

impl UiTheme {
    fn load() -> Self {
        // Try to load from Omarchy theme
        if let Some(colors) = theme::load_omarchy_colors() {
            let accent = colors.accent.as_ref()
                .map(|c| hex_to_color(c))
                .unwrap_or(Color::Rgb(203, 166, 247)); // mauve fallback
            
            let background = colors.background.as_ref()
                .map(|c| hex_to_color(c))
                .unwrap_or(Color::Rgb(49, 50, 68)); // surface0 fallback
            
            let text = colors.foreground.as_ref()
                .map(|c| hex_to_color(c))
                .unwrap_or(Color::Rgb(205, 214, 244)); // text fallback
            
            Self {
                accent,
                secondary: accent, // Use accent as secondary too
                background,
                text,
                success: Color::Rgb(166, 227, 161),  // Keep green
                warning: Color::Rgb(249, 226, 175),  // Keep yellow
            }
        } else {
            // Catppuccin Mocha defaults
            Self {
                accent: Color::Rgb(203, 166, 247),   // mauve
                secondary: Color::Rgb(137, 180, 250), // blue
                background: Color::Rgb(49, 50, 68),  // surface0
                text: Color::Rgb(205, 214, 244),     // text
                success: Color::Rgb(166, 227, 161),  // green
                warning: Color::Rgb(249, 226, 175),  // yellow
            }
        }
    }
}

/// Color presets with hex values
const COLOR_PRESETS: &[(&str, &str)] = &[
    ("White", "ffffff"),
    ("Red", "ff0000"),
    ("Green", "00ff00"),
    ("Blue", "0000ff"),
    ("Cyan", "00ffff"),
    ("Magenta", "ff00ff"),
    ("Yellow", "ffff00"),
    ("Orange", "ff6600"),
    ("Pink", "ff1493"),
    ("Catppuccin Blue", "89b4fa"),
    ("Catppuccin Mauve", "cba6f7"),
    ("Catppuccin Green", "a6e3a1"),
    ("Catppuccin Red", "f38ba8"),
    ("Catppuccin Peach", "fab387"),
    ("Catppuccin Yellow", "f9e2af"),
    ("Catppuccin Teal", "94e2d5"),
];

const THICKNESS_PRESETS: &[(&str, u32)] = &[
    ("Subtle", 40),
    ("Normal", 80),
    ("Strong", 120),
    ("Maximum", 160),
];

const ANIMATION_PRESETS: &[(&str, &str)] = &[
    ("None - Static ring", "none"),
    ("Pulse - Pulsing glow", "pulse"),
    ("Rainbow - Cycling colors", "rainbow"),
    ("Breathe - Gentle breathing", "breathe"),
];

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Main,
    Color,
    Thickness,
    Opacity,
    Glow,
    CornerRadius,
    Animation,
    AnimationSpeed,
    BarHeight,
    BarPosition,
    Monitors,
}

struct App {
    config: Config,
    screen: Screen,
    selected: usize,
    message: Option<String>,
    should_quit: bool,
    input_buffer: String,
    input_mode: bool,
    live_mode: bool, // true if connected to running instance
    monitors: Vec<MonitorState>, // cached monitors list
    visible: bool, // ring light visibility
    theme: UiTheme, // UI color theme
}

impl App {
    fn new() -> Self {
        let live_mode = ipc::is_running();
        let (config, visible) = if live_mode {
            // Try to get current state from running instance
            if let Ok(Some(state)) = ipc::send_command(&Command::GetState) {
                (Config {
                    color: state.color,
                    thickness: state.thickness,
                    opacity: state.opacity,
                    glow: state.glow,
                    corner_radius: state.corner_radius,
                    animation: state.animation,
                    animation_speed: state.animation_speed,
                    ..Config::default()
                }, state.visible)
            } else {
                (Config::load(), true)
            }
        } else {
            (Config::load(), true)
        };
        
        // Get monitors if live
        let monitors = if live_mode {
            ipc::get_monitors().unwrap_or_default()
        } else {
            Vec::new()
        };
        
        Self {
            config,
            screen: Screen::Main,
            selected: 0,
            message: if live_mode {
                Some("Live preview mode - changes apply instantly!".to_string())
            } else {
                Some("Offline mode - start hypr-ringlight first for live preview".to_string())
            },
            should_quit: false,
            input_buffer: String::new(),
            input_mode: false,
            live_mode,
            monitors,
            visible,
            theme: UiTheme::load(),
        }
    }

    fn refresh_monitors(&mut self) {
        if self.live_mode {
            self.monitors = ipc::get_monitors().unwrap_or_default();
        }
    }

    fn main_menu_items(&self) -> Vec<String> {
        let toggle_label = if self.visible { 
            "Ring Light: ON" 
        } else { 
            "Ring Light: OFF" 
        };
        vec![
            toggle_label.to_string(),
            "─────────────────".to_string(),
            "Color".to_string(),
            "Thickness".to_string(), 
            "Opacity".to_string(),
            "Glow".to_string(),
            "Corner Radius".to_string(),
            "Animation".to_string(),
            "Animation Speed".to_string(),
            "Bar Height".to_string(),
            "Bar Position".to_string(),
            "Monitors".to_string(),
            "─────────────────".to_string(),
            "Save Config".to_string(),
            "Exit".to_string(),
        ]
    }

    /// Send update to running instance (if live mode)
    fn send_live_update(&mut self) {
        if !self.live_mode {
            return;
        }
        
        // Send all current values
        let _ = ipc::send_command(&Command::SetColor(self.config.color.clone()));
        let _ = ipc::send_command(&Command::SetThickness(self.config.thickness));
        let _ = ipc::send_command(&Command::SetOpacity(self.config.opacity));
        let _ = ipc::send_command(&Command::SetGlow(self.config.glow));
        let _ = ipc::send_command(&Command::SetCornerRadius(self.config.corner_radius));
        let _ = ipc::send_command(&Command::SetAnimation(self.config.animation.clone()));
        let _ = ipc::send_command(&Command::SetAnimationSpeed(self.config.animation_speed));
    }

    fn handle_input(&mut self, key: KeyCode) {
        if self.input_mode {
            match key {
                KeyCode::Enter => {
                    self.apply_input();
                    self.input_mode = false;
                    self.input_buffer.clear();
                    self.send_live_update();
                }
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.input_buffer.clear();
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                _ => {}
            }
            return;
        }

        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.screen == Screen::Main {
                    self.should_quit = true;
                } else {
                    self.screen = Screen::Main;
                    self.selected = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    // Skip separators (at index 1 and 12)
                    if self.screen == Screen::Main && (self.selected == 1 || self.selected == 12) {
                        if self.selected == 1 {
                            self.selected = 0;
                        } else {
                            self.selected = 11;
                        }
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.max_items();
                if self.selected < max - 1 {
                    self.selected += 1;
                    // Skip separators (at index 1 and 12)
                    if self.screen == Screen::Main && (self.selected == 1 || self.selected == 12) {
                        self.selected += 1;
                    }
                }
            }
            KeyCode::Enter => {
                self.select_item();
            }
            _ => {}
        }
    }

    fn max_items(&self) -> usize {
        match self.screen {
            Screen::Main => 15, // toggle + sep + 10 options + sep + save + exit
            Screen::Color => COLOR_PRESETS.len() + 1, // +1 for custom
            Screen::Thickness => THICKNESS_PRESETS.len() + 1,
            Screen::Animation => ANIMATION_PRESETS.len(),
            Screen::Opacity | Screen::Glow | Screen::CornerRadius | 
            Screen::AnimationSpeed | Screen::BarHeight => 5,
            Screen::BarPosition => 4,
            Screen::Monitors => self.monitors.len().max(1), // at least 1 for "no monitors" message
        }
    }

    fn select_item(&mut self) {
        match self.screen {
            Screen::Main => {
                match self.selected {
                    0 => { // Toggle visibility
                        self.visible = !self.visible;
                        if self.live_mode {
                            let _ = ipc::send_command(&Command::SetVisible(self.visible));
                        }
                        self.message = Some(format!("Ring Light {}", if self.visible { "ON" } else { "OFF" }));
                    }
                    2 => { self.screen = Screen::Color; self.selected = 0; }
                    3 => { self.screen = Screen::Thickness; self.selected = 0; }
                    4 => { self.screen = Screen::Opacity; self.selected = 0; }
                    5 => { self.screen = Screen::Glow; self.selected = 0; }
                    6 => { self.screen = Screen::CornerRadius; self.selected = 0; }
                    7 => { self.screen = Screen::Animation; self.selected = 0; }
                    8 => { self.screen = Screen::AnimationSpeed; self.selected = 0; }
                    9 => { self.screen = Screen::BarHeight; self.selected = 0; }
                    10 => { self.screen = Screen::BarPosition; self.selected = 0; }
                    11 => { // Monitors
                        if self.live_mode {
                            self.refresh_monitors();
                            self.screen = Screen::Monitors; 
                            self.selected = 0;
                        } else {
                            self.message = Some("Monitors only available in live mode".to_string());
                        }
                    }
                    13 => { // Save Config
                        if let Err(e) = self.config.save() {
                            self.message = Some(format!("Error: {}", e));
                        } else {
                            self.message = Some(format!("Saved to {}", Config::path().display()));
                        }
                    }
                    14 => { self.should_quit = true; }
                    _ => {}
                }
            }
            Screen::Color => {
                if self.selected < COLOR_PRESETS.len() {
                    self.config.color = COLOR_PRESETS[self.selected].1.to_string();
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    // Custom input
                    self.input_mode = true;
                    self.input_buffer = self.config.color.clone();
                }
            }
            Screen::Thickness => {
                if self.selected < THICKNESS_PRESETS.len() {
                    self.config.thickness = THICKNESS_PRESETS[self.selected].1;
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.thickness.to_string();
                }
            }
            Screen::Opacity => {
                let values = [0.25, 0.5, 0.75, 1.0];
                if self.selected < 4 {
                    self.config.opacity = values[self.selected];
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.opacity.to_string();
                }
            }
            Screen::Glow => {
                let values = [40, 80, 120, 160];
                if self.selected < 4 {
                    self.config.glow = values[self.selected];
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.glow.to_string();
                }
            }
            Screen::CornerRadius => {
                let values = [1.0, 2.5, 4.0, 6.0];
                if self.selected < 4 {
                    self.config.corner_radius = values[self.selected];
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.corner_radius.to_string();
                }
            }
            Screen::Animation => {
                self.config.animation = ANIMATION_PRESETS[self.selected].1.to_string();
                self.send_live_update();
                self.screen = Screen::Main;
                self.selected = 0;
            }
            Screen::AnimationSpeed => {
                let values = [60, 120, 240, 480];
                if self.selected < 4 {
                    self.config.animation_speed = values[self.selected];
                    self.send_live_update();
                    self.screen = Screen::Main;
                    self.selected = 0;
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.animation_speed.to_string();
                }
            }
            Screen::BarHeight => {
                let values = [0, 25, 35, 45];
                if self.selected < 4 {
                    self.config.bar_height = values[self.selected];
                    self.screen = Screen::Main;
                    self.selected = 0;
                    self.message = Some("Bar height requires restart to apply".to_string());
                } else {
                    self.input_mode = true;
                    self.input_buffer = self.config.bar_height.to_string();
                }
            }
            Screen::BarPosition => {
                let positions = ["top", "bottom", "left", "right"];
                self.config.bar_position = positions[self.selected].to_string();
                self.screen = Screen::Main;
                self.selected = 0;
                self.message = Some("Bar position requires restart to apply".to_string());
            }
            Screen::Monitors => {
                if !self.monitors.is_empty() && self.selected < self.monitors.len() {
                    let monitor = &self.monitors[self.selected];
                    let new_enabled = !monitor.enabled;
                    let id = monitor.id.clone();
                    
                    // Send command to toggle
                    if let Err(e) = ipc::set_monitor_enabled(&id, new_enabled) {
                        self.message = Some(format!("Error: {}", e));
                    } else {
                        // Refresh local state
                        self.refresh_monitors();
                        self.message = Some(format!(
                            "{} {}",
                            if new_enabled { "Enabled" } else { "Disabled" },
                            self.monitors.get(self.selected).map(|m| m.display_name.as_str()).unwrap_or(&id)
                        ));
                    }
                }
            }
        }
    }

    fn apply_input(&mut self) {
        match self.screen {
            Screen::Color => {
                self.config.color = self.input_buffer.trim_start_matches('#').to_string();
            }
            Screen::Thickness => {
                if let Ok(v) = self.input_buffer.parse() {
                    self.config.thickness = v;
                }
            }
            Screen::Opacity => {
                if let Ok(v) = self.input_buffer.parse::<f64>() {
                    self.config.opacity = v.clamp(0.0, 1.0);
                }
            }
            Screen::Glow => {
                if let Ok(v) = self.input_buffer.parse() {
                    self.config.glow = v;
                }
            }
            Screen::CornerRadius => {
                if let Ok(v) = self.input_buffer.parse() {
                    self.config.corner_radius = v;
                }
            }
            Screen::AnimationSpeed => {
                if let Ok(v) = self.input_buffer.parse() {
                    self.config.animation_speed = v;
                }
            }
            Screen::BarHeight => {
                if let Ok(v) = self.input_buffer.parse() {
                    self.config.bar_height = v;
                }
            }
            _ => {}
        }
        self.screen = Screen::Main;
        self.selected = 0;
    }
}

fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::Rgb(r, g, b)
    } else {
        Color::White
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    // Use theme colors from Omarchy or defaults
    let accent = app.theme.accent;
    let secondary = app.theme.secondary;
    let background = app.theme.background;
    let text = app.theme.text;
    let success = app.theme.success;
    let warning = app.theme.warning;
    
    // Clear background
    frame.render_widget(Block::default().style(Style::default().bg(background)), area);
    
    // Layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(12), // Current settings
            Constraint::Min(10),   // Menu
            Constraint::Length(2), // Help
        ])
        .split(area);
    
    // Title with live mode indicator
    let title_text = if app.live_mode {
        "hypr-ringlight configurator [LIVE]"
    } else {
        "hypr-ringlight configurator [OFFLINE]"
    };
    let title_color = if app.live_mode { success } else { warning };
    
    let title = Paragraph::new(title_text)
        .style(Style::default().fg(title_color).bold())
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(secondary)));
    frame.render_widget(title, chunks[0]);
    
    // Current settings with color preview
    let color_preview = "██".to_string();
    let settings_text = vec![
        Line::from(vec![
            Span::styled("Color:          ", Style::default().fg(text)),
            Span::styled(format!("#{} ", app.config.color), Style::default().fg(text)),
            Span::styled(color_preview, Style::default().fg(hex_to_color(&app.config.color))),
        ]),
        Line::from(vec![
            Span::styled("Thickness:      ", Style::default().fg(text)),
            Span::styled(format!("{}px", app.config.thickness), Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Opacity:        ", Style::default().fg(text)),
            Span::styled(format!("{}", app.config.opacity), Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Glow:           ", Style::default().fg(text)),
            Span::styled(format!("{}px", app.config.glow), Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Corner Radius:  ", Style::default().fg(text)),
            Span::styled(format!("{}x", app.config.corner_radius), Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Animation:      ", Style::default().fg(text)),
            Span::styled(&app.config.animation, Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Anim Speed:     ", Style::default().fg(text)),
            Span::styled(format!("{}", app.config.animation_speed), Style::default().fg(success)),
        ]),
        Line::from(vec![
            Span::styled("Bar:            ", Style::default().fg(text)),
            Span::styled(format!("{}px @ {}", app.config.bar_height, app.config.bar_position), Style::default().fg(success)),
        ]),
    ];
    
    let settings = Paragraph::new(settings_text)
        .block(Block::default()
            .title(" Current Settings ")
            .title_style(Style::default().fg(accent).bold())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(secondary)));
    frame.render_widget(settings, chunks[1]);
    
    // Menu area
    let menu_title = match app.screen {
        Screen::Main => " Menu ",
        Screen::Color => " Select Color ",
        Screen::Thickness => " Select Thickness ",
        Screen::Opacity => " Select Opacity ",
        Screen::Glow => " Select Glow ",
        Screen::CornerRadius => " Select Corner Radius ",
        Screen::Animation => " Select Animation ",
        Screen::AnimationSpeed => " Select Animation Speed ",
        Screen::BarHeight => " Select Bar Height ",
        Screen::BarPosition => " Select Bar Position ",
        Screen::Monitors => " Monitors (Enter to toggle) ",
    };
    
    let items: Vec<ListItem> = match app.screen {
        Screen::Main => {
            let menu_items = app.main_menu_items();
            menu_items.iter().enumerate().map(|(i, item)| {
                let is_toggle = i == 0;
                let is_separator = item.starts_with('─');
                
                if is_toggle {
                    // Special styling for ON/OFF toggle
                    let (status, status_color) = if app.visible {
                        ("ON", success)
                    } else {
                        ("OFF", Color::Red)
                    };
                    let base_style = if i == app.selected {
                        Style::default().fg(background).bg(accent).bold()
                    } else {
                        Style::default().fg(text)
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(" Ring Light: "),
                        Span::styled(status, Style::default().fg(status_color).bold()),
                    ])).style(base_style)
                } else if is_separator {
                    ListItem::new(format!(" {} ", item)).style(Style::default().fg(Color::DarkGray))
                } else {
                    let style = if i == app.selected {
                        Style::default().fg(background).bg(accent).bold()
                    } else {
                        Style::default().fg(text)
                    };
                    ListItem::new(format!(" {} ", item)).style(style)
                }
            }).collect()
        }
        Screen::Color => {
            let mut items: Vec<ListItem> = COLOR_PRESETS.iter().enumerate().map(|(i, (name, hex))| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                let color_block = Span::styled("██ ", Style::default().fg(hex_to_color(hex)));
                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    color_block,
                    Span::styled(format!("{} (#{hex})", name), style),
                ]))
            }).collect();
            
            let custom_style = if app.selected == COLOR_PRESETS.len() {
                Style::default().fg(background).bg(accent).bold()
            } else {
                Style::default().fg(text)
            };
            items.push(ListItem::new(" ✎  Custom hex code...").style(custom_style));
            items
        }
        Screen::Thickness => {
            let mut items: Vec<ListItem> = THICKNESS_PRESETS.iter().enumerate().map(|(i, (name, val))| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {} ({}px)", name, val)).style(style)
            }).collect();
            
            let custom_style = if app.selected == THICKNESS_PRESETS.len() {
                Style::default().fg(background).bg(accent).bold()
            } else {
                Style::default().fg(text)
            };
            items.push(ListItem::new(" ✎  Custom...").style(custom_style));
            items
        }
        Screen::Opacity => {
            ["25%", "50%", "75%", "100%", "✎  Custom..."].iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::Glow => {
            ["Subtle (40px)", "Normal (80px)", "Strong (120px)", "Maximum (160px)", "✎  Custom..."]
                .iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::CornerRadius => {
            ["Sharp (1.0x)", "Normal (2.5x)", "Round (4.0x)", "Very Round (6.0x)", "✎  Custom..."]
                .iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::Animation => {
            ANIMATION_PRESETS.iter().enumerate().map(|(i, (name, _))| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", name)).style(style)
            }).collect()
        }
        Screen::AnimationSpeed => {
            ["Fast (60)", "Normal (120)", "Slow (240)", "Very Slow (480)", "✎  Custom..."]
                .iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::BarHeight => {
            ["None (0px)", "Small (25px)", "Normal (35px)", "Large (45px)", "✎  Custom..."]
                .iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::BarPosition => {
            ["Top", "Bottom", "Left", "Right"].iter().enumerate().map(|(i, item)| {
                let style = if i == app.selected {
                    Style::default().fg(background).bg(accent).bold()
                } else {
                    Style::default().fg(text)
                };
                ListItem::new(format!(" {}", item)).style(style)
            }).collect()
        }
        Screen::Monitors => {
            if app.monitors.is_empty() {
                vec![ListItem::new(" No monitors detected (is hypr-ringlight running?)").style(Style::default().fg(warning))]
            } else {
                app.monitors.iter().enumerate().map(|(i, m)| {
                    let status = if m.enabled { "[ON] " } else { "[OFF]" };
                    let status_color = if m.enabled { success } else { Color::Red };
                    let style = if i == app.selected {
                        Style::default().fg(background).bg(accent).bold()
                    } else {
                        Style::default().fg(text)
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(status, Style::default().fg(status_color).bold()),
                        Span::raw(" "),
                        Span::styled(format!("{} ({})", m.display_name, m.id), style),
                    ]))
                }).collect()
            }
        }
    };
    
    let menu = List::new(items)
        .block(Block::default()
            .title(menu_title)
            .title_style(Style::default().fg(accent).bold())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(secondary)));
    frame.render_widget(menu, chunks[2]);
    
    // Help text or input mode
    let help_text = if app.input_mode {
        format!(" Input: {}█  [Enter] confirm  [Esc] cancel", app.input_buffer)
    } else if let Some(ref msg) = app.message {
        format!(" {}", msg)
    } else {
        " [↑↓/jk] navigate  [Enter] select  [Esc/q] back/quit".to_string()
    };
    
    let help_style = if app.input_mode {
        Style::default().fg(success).bold()
    } else if app.message.is_some() {
        Style::default().fg(success)
    } else {
        Style::default().fg(text)
    };
    
    let help = Paragraph::new(help_text).style(help_style);
    frame.render_widget(help, chunks[3]);
}

pub fn run() -> Result<(), String> {
    // Setup terminal
    enable_raw_mode().map_err(|e| e.to_string())?;
    stdout().execute(EnterAlternateScreen).map_err(|e| e.to_string())?;
    
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))
        .map_err(|e| e.to_string())?;
    
    let mut app = App::new();
    
    // Main loop
    loop {
        terminal.draw(|f| draw(f, &app)).map_err(|e| e.to_string())?;
        
        if event::poll(std::time::Duration::from_millis(100)).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind == KeyEventKind::Press {
                    // Clear message on any keypress
                    app.message = None;
                    app.handle_input(key.code);
                }
            }
        }
        
        if app.should_quit {
            break;
        }
    }
    
    // Restore terminal
    disable_raw_mode().map_err(|e| e.to_string())?;
    stdout().execute(LeaveAlternateScreen).map_err(|e| e.to_string())?;
    
    Ok(())
}
