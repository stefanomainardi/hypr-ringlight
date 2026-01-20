mod camera;
mod config;
mod ipc;
mod theme;
mod tui;

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use clap::{Parser, Subcommand};
use ksni::{menu::StandardItem, menu::SubMenu, menu::RadioGroup, menu::RadioItem, menu::CheckmarkItem, Tray, TrayService};
use signal_hook::consts::SIGUSR2;
use signal_hook::iterator::Signals;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
    Connection, QueueHandle, Proxy,
};

use config::{Config, BarPosition};
use ipc::IpcState;

/// Ring Light overlay for Hyprland/Wayland
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Ring color in hex format (e.g., ff0000 for red)
    #[arg(short, long)]
    color: Option<String>,

    /// Ring thickness in pixels
    #[arg(short, long)]
    thickness: Option<u32>,

    /// Ring opacity (0.0 - 1.0)
    #[arg(short, long)]
    opacity: Option<f64>,

    /// Blur/glow radius (softness)
    #[arg(short, long)]
    glow: Option<u32>,

    /// Corner radius multiplier (relative to thickness)
    #[arg(long)]
    corner_radius: Option<f64>,

    /// Animation mode (none, pulse, rainbow, breathe)
    #[arg(short, long)]
    animation: Option<String>,

    /// Animation speed (frames per cycle, lower = faster)
    #[arg(long)]
    animation_speed: Option<u32>,

    /// Waybar/bar height in pixels (ring starts below/beside this)
    #[arg(long)]
    bar_height: Option<u32>,

    /// Waybar/bar position (top, bottom, left, right)
    #[arg(long)]
    bar_position: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Interactive configuration TUI (live preview)
    Config,
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

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0) as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;

    let hue_to_rgb = |p: f64, q: f64, mut t: f64| -> f64 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0 / 2.0 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };

    (
        (hue_to_rgb(p, q, h + 1.0 / 3.0) * 255.0) as u8,
        (hue_to_rgb(p, q, h) * 255.0) as u8,
        (hue_to_rgb(p, q, h - 1.0 / 3.0) * 255.0) as u8,
    )
}

/// Monitor info for tray menu (id + display name + enabled status)
#[derive(Clone, Debug)]
struct MonitorInfo {
    id: String,           // Connector name (DP-2, HDMI-1, etc.) - used as unique ID
    display_name: String, // Friendly name (brand/model) - shown in UI
    enabled: bool,
}

/// Extended shared state with IPC support
struct SharedState {
    ipc: Arc<IpcState>,
}

impl SharedState {
    fn new(
        color: (u8, u8, u8),
        thickness: u32,
        opacity: f64,
        glow: u32,
        corner_radius: f64,
        animation: u8,
        animation_speed: u32,
        disabled_monitors: Vec<String>,
    ) -> Self {
        Self {
            ipc: Arc::new(IpcState::new(color, thickness, opacity, glow, corner_radius, animation, animation_speed, disabled_monitors)),
        }
    }
    
    fn toggle_monitor(&self, id: &str) {
        self.ipc.toggle_monitor(id);
        self.ipc.save_to_config();
    }
    
    fn is_monitor_enabled(&self, id: &str) -> bool {
        self.ipc.is_monitor_enabled(id)
    }
    
    fn add_monitor(&self, id: String, display_name: String) {
        self.ipc.add_monitor(id, display_name);
    }
    
    fn remove_monitor(&self, id: &str) {
        self.ipc.remove_monitor(id);
    }
    
    fn get_monitors(&self) -> Vec<MonitorInfo> {
        self.ipc.get_monitors().into_iter().map(|m| MonitorInfo {
            id: m.id,
            display_name: m.display_name,
            enabled: m.enabled,
        }).collect()
    }
}

// Tray icon
struct RingLightTray {
    state: Arc<SharedState>,
}

impl Tray for RingLightTray {
    fn id(&self) -> String {
        "hypr-ringlight".into()
    }

    fn icon_name(&self) -> String {
        "video-display".into()
    }

    fn title(&self) -> String {
        "RingLight".into()
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let is_visible = self.state.ipc.is_visible();
        let current_anim = self.state.ipc.get_animation_mode();
        let current_thickness = self.state.ipc.get_thickness();
        let monitors = self.state.get_monitors();
        
        // Map thickness to preset index
        let thickness_idx = match current_thickness {
            40 => 0,
            80 => 1,
            120 => 2,
            160 => 3,
            _ => 4,
        };

        let mut menu = vec![
            // Show/Hide toggle
            StandardItem {
                label: if is_visible { "Hide Ring" } else { "Show Ring" }.into(),
                activate: Box::new(|tray: &mut Self| {
                    let current = tray.state.ipc.is_visible();
                    tray.state.ipc.visible.store(!current, Ordering::Relaxed);
                    tray.state.ipc.save_to_config();
                }),
                ..Default::default()
            }.into(),
            
            ksni::MenuItem::Separator,
            
            // Width submenu
            SubMenu {
                label: format!("Width ({}px)", current_thickness),
                submenu: vec![
                    RadioGroup {
                        selected: thickness_idx,
                        select: Box::new(|tray: &mut Self, idx| {
                            let val = match idx {
                                0 => 40,
                                1 => 80,
                                2 => 120,
                                3 => 160,
                                _ => return,
                            };
                            tray.state.ipc.thickness.store(val, Ordering::Relaxed);
                            tray.state.ipc.save_to_config();
                        }),
                        options: vec![
                            RadioItem { label: "Subtle (40px)".into(), ..Default::default() },
                            RadioItem { label: "Normal (80px)".into(), ..Default::default() },
                            RadioItem { label: "Strong (120px)".into(), ..Default::default() },
                            RadioItem { label: "Maximum (160px)".into(), ..Default::default() },
                        ],
                    }.into(),
                    ksni::MenuItem::Separator,
                    StandardItem {
                        label: "Increase (+20px)".into(),
                        icon_name: "list-add-symbolic".into(),
                        activate: Box::new(|tray: &mut Self| {
                            let current = tray.state.ipc.get_thickness();
                            tray.state.ipc.thickness.store((current + 20).min(200), Ordering::Relaxed);
                            tray.state.ipc.save_to_config();
                        }),
                        ..Default::default()
                    }.into(),
                    StandardItem {
                        label: "Decrease (-20px)".into(),
                        icon_name: "list-remove-symbolic".into(),
                        activate: Box::new(|tray: &mut Self| {
                            let current = tray.state.ipc.get_thickness();
                            tray.state.ipc.thickness.store(current.saturating_sub(20).max(10), Ordering::Relaxed);
                            tray.state.ipc.save_to_config();
                        }),
                        ..Default::default()
                    }.into(),
                ],
                ..Default::default()
            }.into(),
            
            // Animation submenu
            SubMenu {
                label: format!("Animation ({})", match current_anim {
                    0 => "None",
                    1 => "Pulse", 
                    2 => "Rainbow",
                    3 => "Breathe",
                    _ => "Unknown",
                }),
                submenu: vec![
                    RadioGroup {
                        selected: current_anim as usize,
                        select: Box::new(|tray: &mut Self, idx| {
                            tray.state.ipc.animation_mode.store(idx as u8, Ordering::Relaxed);
                            tray.state.ipc.save_to_config();
                        }),
                        options: vec![
                            RadioItem { label: "None".into(), ..Default::default() },
                            RadioItem { label: "Pulse".into(), ..Default::default() },
                            RadioItem { label: "Rainbow".into(), ..Default::default() },
                            RadioItem { label: "Breathe".into(), ..Default::default() },
                        ],
                    }.into(),
                ],
                ..Default::default()
            }.into(),
        ];
        
        // Monitors submenu (only if we have monitors)
        if !monitors.is_empty() {
            let enabled_count = monitors.iter().filter(|m| m.enabled).count();
            let monitor_items: Vec<ksni::MenuItem<Self>> = monitors.iter().map(|m| {
                let id = m.id.clone();
                let label = if m.enabled {
                    format!("[ON]  {}", m.display_name)
                } else {
                    format!("[OFF] {}", m.display_name)
                };
                CheckmarkItem {
                    label,
                    checked: m.enabled,
                    activate: Box::new(move |tray: &mut Self| {
                        tray.state.toggle_monitor(&id);
                    }),
                    ..Default::default()
                }.into()
            }).collect();
            
            menu.push(SubMenu {
                label: format!("Monitors ({}/{})", enabled_count, monitors.len()),
                submenu: monitor_items,
                ..Default::default()
            }.into());
        }
        
        menu.push(ksni::MenuItem::Separator);
        
        // Quit
        menu.push(StandardItem {
            label: "Quit".into(),
            activate: Box::new(|_| {
                std::process::exit(0);
            }),
            ..Default::default()
        }.into());
        
        menu
    }
}

/// State for a single monitor's ring light
struct MonitorRing {
    layer: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    first_configure: bool,
    output_name: String,
}

struct RingLight {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    
    /// Map from wl_surface id to monitor ring
    monitors: HashMap<u32, MonitorRing>,
    /// Map from wl_output id to output name
    output_names: HashMap<u32, String>,
    
    start_time: Instant,
    
    // Static config (bar position can't change at runtime)
    bar_height: i32,
    bar_position: BarPosition,
    
    // Shared state with tray and IPC
    state: Arc<SharedState>,
}

impl RingLight {
    fn create_ring_for_output(&mut self, qh: &QueueHandle<Self>, output: &wl_output::WlOutput, id: String, display_name: String) {
        // Create surface
        let surface = self.compositor.create_surface(qh);
        
        // Create empty input region for click-through
        let empty_region = Region::new(&self.compositor).expect("Failed to create region");
        surface.set_input_region(Some(empty_region.wl_region()));

        // Create layer surface bound to this specific output
        let layer = self.layer_shell.create_layer_surface(
            qh, 
            surface.clone(), 
            Layer::Overlay, 
            Some("ringlight"), 
            Some(output)
        );
        
        // Configure
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        
        // Set margin for bar
        match self.bar_position {
            BarPosition::Top => layer.set_margin(self.bar_height, 0, 0, 0),
            BarPosition::Bottom => layer.set_margin(0, 0, self.bar_height, 0),
            BarPosition::Left => layer.set_margin(0, 0, 0, self.bar_height),
            BarPosition::Right => layer.set_margin(0, self.bar_height, 0, 0),
        }

        layer.commit();

        // Create buffer pool
        let pool = SlotPool::new(1920 * 1080 * 4, &self.shm).expect("Failed to create pool");
        
        let surface_id = surface.id().protocol_id();
        
        // Add to shared state
        self.state.add_monitor(id.clone(), display_name);

        self.monitors.insert(surface_id, MonitorRing {
            layer,
            pool,
            width: 0,
            height: 0,
            first_configure: true,
            output_name: id,
        });
    }
    
    fn draw_monitor(&mut self, surface_id: u32, qh: &QueueHandle<Self>) {
        let monitor = match self.monitors.get_mut(&surface_id) {
            Some(m) => m,
            None => return,
        };
        
        let width = monitor.width;
        let height = monitor.height;
        
        if width == 0 || height == 0 {
            return;
        }
        
        // Check if this monitor is enabled
        let monitor_enabled = self.state.is_monitor_enabled(&monitor.output_name);

        let stride = width as i32 * 4;
        let (buffer, canvas) = monitor
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create buffer");

        // Read all values from IpcState (allows real-time updates)
        let is_visible = self.state.ipc.is_visible() && monitor_enabled;
        let anim_mode = self.state.ipc.get_animation_mode();
        let thickness = self.state.ipc.get_thickness() as f64;
        let glow = self.state.ipc.get_glow() as f64;
        let corner_radius = thickness * self.state.ipc.get_corner_radius();
        let base_color = self.state.ipc.get_color();
        let base_opacity = self.state.ipc.get_opacity();
        let animation_speed = self.state.ipc.get_animation_speed();
        
        // Animation frame
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let frame = (elapsed * 60.0) as u32;
        
        // Calculate animated color and opacity
        let (color, opacity) = if !is_visible {
            ((0, 0, 0), 0.0)
        } else {
            match anim_mode {
                0 => (base_color, base_opacity),
                1 => {
                    let pulse = ((frame as f64 / animation_speed as f64) * 2.0 * std::f64::consts::PI).sin();
                    let opacity = base_opacity * (0.5 + 0.5 * pulse);
                    (base_color, opacity)
                }
                2 => {
                    let hue = (frame as f64 / animation_speed as f64) % 1.0;
                    let color = hsl_to_rgb(hue, 1.0, 0.5);
                    (color, base_opacity)
                }
                3 => {
                    let breathe = ((frame as f64 / animation_speed as f64) * std::f64::consts::PI).sin();
                    let opacity = base_opacity * breathe.abs().max(0.1);
                    (base_color, opacity)
                }
                _ => (base_color, base_opacity),
            }
        };

        // Draw pixels
        canvas.chunks_exact_mut(4).enumerate().for_each(|(index, chunk)| {
            let x = (index % width as usize) as f64;
            let y = (index / width as usize) as f64;
            let w = width as f64;
            let h = height as f64;

            let total_ring = thickness + glow;
            let dist_to_inner = distance_to_inner_rounded_border(x, y, w, h, total_ring, corner_radius);
            
            let alpha = if dist_to_inner <= 0.0 {
                0.0
            } else if dist_to_inner > glow {
                opacity
            } else {
                let glow_progress = dist_to_inner / glow;
                let smooth = glow_progress * glow_progress * glow_progress;
                opacity * smooth
            };

            if alpha > 0.001 {
                let a = (alpha * 255.0) as u32;
                let (r, g, b) = color;
                let pr = ((r as u32) * a / 255) as u8;
                let pg = ((g as u32) * a / 255) as u8;
                let pb = ((b as u32) * a / 255) as u8;
                let pixel = (a << 24) | ((pr as u32) << 16) | ((pg as u32) << 8) | (pb as u32);
                chunk.copy_from_slice(&pixel.to_ne_bytes());
            } else {
                chunk.copy_from_slice(&[0, 0, 0, 0]);
            }
        });

        // Damage and commit
        monitor.layer.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
        monitor.layer.wl_surface().frame(qh, monitor.layer.wl_surface().clone());
        buffer.attach_to(monitor.layer.wl_surface()).expect("buffer attach");
        monitor.layer.commit();
    }
}

/// Calculate signed distance from a point to the inner rounded rectangle border.
fn distance_to_inner_rounded_border(x: f64, y: f64, w: f64, h: f64, inset: f64, corner_radius: f64) -> f64 {
    let left = inset;
    let right = w - inset;
    let top = inset;
    let bottom = h - inset;
    
    if right <= left || bottom <= top {
        return 100.0;
    }
    
    let half_w = (right - left) / 2.0;
    let half_h = (bottom - top) / 2.0;
    let r = corner_radius.min(half_w).min(half_h).max(0.0);
    
    let cx = (left + right) / 2.0;
    let cy = (top + bottom) / 2.0;
    let half_width = (right - left) / 2.0;
    let half_height = (bottom - top) / 2.0;
    
    let px = (x - cx).abs();
    let py = (y - cy).abs();
    
    let qx = px - (half_width - r);
    let qy = py - (half_height - r);
    
    let outside_dist = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside_dist = qx.max(qy).min(0.0);
    let sdf = outside_dist + inside_dist - r;
    
    sdf
}

impl CompositorHandler for RingLight {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {}

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {}

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        let surface_id = surface.id().protocol_id();
        self.draw_monitor(surface_id, qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {}

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {}
}

impl OutputHandler for RingLight {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        // Get output info
        if let Some(info) = self.output_state.info(&output) {
            // Build a friendly name: prefer make, fallback to connector name
            let connector = info.name.clone().unwrap_or_else(|| format!("output-{}", output.id().protocol_id()));
            let display_name = if !info.make.is_empty() {
                // Use make (brand) - e.g. "Dell", "LG", "Samsung"
                if !info.model.is_empty() {
                    format!("{} {}", info.make, info.model)
                } else {
                    info.make.clone()
                }
            } else if !info.model.is_empty() {
                info.model.clone()
            } else {
                connector.clone()
            };
            
            let output_id = output.id().protocol_id();
            
            // Use connector as internal ID (unique), display_name for UI
            self.output_names.insert(output_id, connector.clone());
            self.create_ring_for_output(qh, &output, connector, display_name);
        }
    }
    
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let output_id = output.id().protocol_id();
        if let Some(name) = self.output_names.remove(&output_id) {
            self.state.remove_monitor(&name);
            // Find and remove the monitor ring by name
            self.monitors.retain(|_, m| m.output_name != name);
        }
    }
}

impl LayerShellHandler for RingLight {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let surface_id = layer.wl_surface().id().protocol_id();
        self.monitors.remove(&surface_id);
        
        // Exit if all monitors are gone
        if self.monitors.is_empty() {
            std::process::exit(0);
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let surface_id = layer.wl_surface().id().protocol_id();
        
        if let Some(monitor) = self.monitors.get_mut(&surface_id) {
            monitor.width = configure.new_size.0;
            monitor.height = configure.new_size.1;

            if monitor.first_configure {
                monitor.first_configure = false;
                // Draw will happen in next frame callback
            }
        }
        
        self.draw_monitor(surface_id, &qh);
    }
}

impl ShmHandler for RingLight {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(RingLight);
delegate_output!(RingLight);
delegate_shm!(RingLight);
delegate_layer!(RingLight);
delegate_registry!(RingLight);

impl ProvidesRegistryState for RingLight {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

fn main() {
    env_logger::init();
    
    let cli = Cli::parse();
    
    // Handle subcommands
    if let Some(Commands::Config) = cli.command {
        if let Err(e) = tui::run() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return;
    }
    
    // Load config file, then override with CLI args
    let mut cfg = Config::load();
    
    // Track if color was explicitly set
    let color_explicitly_set = cli.color.is_some();
    
    if let Some(v) = cli.color { cfg.color = v; }
    if let Some(v) = cli.thickness { cfg.thickness = v; }
    if let Some(v) = cli.opacity { cfg.opacity = v; }
    if let Some(v) = cli.glow { cfg.glow = v; }
    if let Some(v) = cli.corner_radius { cfg.corner_radius = v; }
    if let Some(v) = cli.animation { cfg.animation = v; }
    if let Some(v) = cli.animation_speed { cfg.animation_speed = v; }
    if let Some(v) = cli.bar_height { cfg.bar_height = v; }
    if let Some(v) = cli.bar_position { cfg.bar_position = v; }
    
    // If color wasn't explicitly set via CLI and config has default, try Omarchy theme
    let initial_color = if !color_explicitly_set && cfg.color == "ffffff" {
        // Try to get accent color from Omarchy theme
        if let Some(color) = theme::get_accent_color() {
            log::info!("Using Omarchy theme accent color: #{:02x}{:02x}{:02x}", color.0, color.1, color.2);
            color
        } else {
            parse_hex_color(&cfg.color)
        }
    } else {
        parse_hex_color(&cfg.color)
    };
    
    // Create shared state with all config values
    let state = Arc::new(SharedState::new(
        initial_color,
        cfg.thickness,
        cfg.opacity,
        cfg.glow,
        cfg.corner_radius,
        cfg.animation_mode(),
        cfg.animation_speed,
        cfg.disabled_monitors.clone(),
    ));

    // Start IPC server for live config updates
    ipc::start_server(state.ipc.clone());

    // Set up SIGUSR2 handler for Omarchy theme reload
    let signal_state = state.clone();
    std::thread::spawn(move || {
        let mut signals = Signals::new(&[SIGUSR2]).expect("Failed to create signal handler");
        for _ in signals.forever() {
            // Reload theme colors from Omarchy
            if let Some((r, g, b)) = theme::get_accent_color() {
                signal_state.ipc.set_color(r, g, b);
                log::info!("Reloaded Omarchy theme color: #{:02x}{:02x}{:02x}", r, g, b);
            }
        }
    });

    // Connect to Wayland
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, mut event_queue) = registry_queue_init(&conn).expect("Failed to init registry");
    let qh = event_queue.handle();

    // Bind globals
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("layer shell not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm not available");

    let mut ring_light = RingLight {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        monitors: HashMap::new(),
        output_names: HashMap::new(),
        start_time: Instant::now(),
        bar_height: cfg.bar_height as i32,
        bar_position: cfg.bar_position_enum(),
        state: state.clone(),
    };

    // Initial roundtrip to get output info
    event_queue.roundtrip(&mut ring_light).expect("Initial roundtrip failed");
    
    // Create rings for all existing outputs
    let outputs: Vec<_> = ring_light.output_state.outputs().collect();
    for output in outputs {
        if let Some(info) = ring_light.output_state.info(&output) {
            let connector = info.name.clone().unwrap_or_else(|| format!("output-{}", output.id().protocol_id()));
            let display_name = if !info.make.is_empty() {
                if !info.model.is_empty() {
                    format!("{} {}", info.make, info.model)
                } else {
                    info.make.clone()
                }
            } else if !info.model.is_empty() {
                info.model.clone()
            } else {
                connector.clone()
            };
            
            let output_id = output.id().protocol_id();
            ring_light.output_names.insert(output_id, connector.clone());
            ring_light.create_ring_for_output(&qh, &output, connector, display_name);
        }
    }

    // Start tray AFTER monitors are discovered
    let tray_state = state.clone();
    std::thread::spawn(move || {
        let service = TrayService::new(RingLightTray {
            state: tray_state,
        });
        let _ = service.run();
    });

    // Start camera monitor for video call notifications
    let camera_visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let camera_visible_ref = camera_visible.clone();
    let camera_state = state.clone();
    std::thread::spawn(move || {
        loop {
            camera_visible_ref.store(camera_state.ipc.is_visible(), Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    camera::start_camera_monitor(camera_visible);

    // Event loop
    loop {
        event_queue.blocking_dispatch(&mut ring_light).expect("Wayland dispatch failed");
    }
}
