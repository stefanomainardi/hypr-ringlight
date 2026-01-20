<h1 align="center">hypr-ringlight</h1>

<p align="center">
  <strong>A customizable ring light overlay for Hyprland/Wayland</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#usage">Usage</a> •
  <a href="#configuration">Configuration</a> •
  <a href="#license">License</a>
</p>

---

**hypr-ringlight** displays a glowing ring light effect around the edges of your screen(s). Inspired by the Edge Light feature introduced in macOS Tahoe.

> **Note**: This project is experimental and under active development.

## Features

- Smooth glow effect with customizable intensity
- Rounded inner corners for a polished look
- **Multi-monitor support** with per-monitor toggle
- **System tray integration** with full control
- **Interactive TUI configurator** with live preview
- **Multiple animations**: None, Pulse, Rainbow, Breathe
- **Click-through**: doesn't block mouse input or scrolling
- Configurable thickness, color, opacity, and glow radius
- Config file support (`~/.config/hypr-ringlight/config.toml`)
- Works with any Wayland compositor supporting `wlr-layer-shell`

## Demo

https://github.com/stefanomainardi/hypr-ringlight/raw/main/hypr-ringlight.mp4

## Requirements

- Hyprland (or any Wayland compositor with `wlr-layer-shell` support)
- System tray (e.g., Waybar with tray module)
- D-Bus (for tray integration)

## Installation

### From source

```bash
# Clone the repository
git clone https://github.com/stefanomainardi/hypr-ringlight.git
cd hypr-ringlight

# Build
cargo build --release

# Run
./target/release/hypr-ringlight
```

### Dependencies

#### Arch Linux
```bash
sudo pacman -S rust dbus
```

#### Debian/Ubuntu
```bash
sudo apt install rustc cargo libdbus-1-dev pkg-config
```

#### Fedora
```bash
sudo dnf install rust cargo dbus-devel
```

## Usage

### Basic

```bash
hypr-ringlight
```

### With custom options

```bash
# Red ring with rainbow animation
hypr-ringlight --color ff0000 --animation rainbow

# Thicker ring with more glow
hypr-ringlight --thickness 120 --glow 100

# Adjust for a bottom bar
hypr-ringlight --bar-position bottom --bar-height 40
```

### Interactive TUI Configurator

```bash
# Open the TUI configurator
hypr-ringlight config
```

The TUI provides an interactive way to configure all ring light parameters with a beautiful Catppuccin-themed interface.

**Controls:**
- `↑/↓` or `j/k` - Navigate options
- `Enter` - Select option
- `Esc` - Go back / Exit
- `q` - Quit

**Live Preview:** If the ring light is already running, changes are applied in real-time! The TUI shows `[LIVE]` when connected or `[OFFLINE]` when the ring light isn't running.

**Save Configuration:** Select "Save & Exit" to persist your settings to the config file.

## Configuration

### Command Line Options

| Option | Default | Description |
|--------|---------|-------------|
| `-c, --color` | `ffffff` | Ring color in hex format (e.g., `ff0000` for red) |
| `-t, --thickness` | `80` | Ring thickness in pixels |
| `-o, --opacity` | `1.0` | Ring opacity (0.0 - 1.0) |
| `-g, --glow` | `80` | Glow/blur radius in pixels |
| `--corner-radius` | `2.5` | Inner corner radius multiplier (relative to thickness) |
| `-a, --animation` | `none` | Animation mode: `none`, `pulse`, `rainbow`, `breathe` |
| `--animation-speed` | `120` | Animation speed (lower = faster) |
| `--bar-height` | `35` | Height of your status bar in pixels |
| `--bar-position` | `top` | Position of your bar: `top`, `bottom`, `left`, `right` |

### Config File

Settings are stored in `~/.config/hypr-ringlight/config.toml`:

```toml
color = "89b4fa"
thickness = 80
opacity = 1.0
glow = 80
corner_radius = 2.5
animation = "none"
animation_speed = 120
bar_height = 35
bar_position = "top"
```

The config file is:
- Created automatically when you save from the TUI
- Loaded on startup (CLI args override config file values)
- Editable manually with any text editor

### Tray Menu

Right-click the tray icon to access:

- **Show/Hide Ring** - Toggle visibility
- **Width** - Preset sizes (Subtle, Normal, Strong, Maximum) and fine adjustment
- **Animation** - Select animation mode
- **Monitors** - Enable/disable ring on individual monitors

## Autostart

### Hyprland

Add to your `~/.config/hypr/hyprland.conf`:

```ini
exec-once = hypr-ringlight
```

### With custom options

```ini
exec-once = hypr-ringlight --color 00ffff --animation breathe --thickness 60
```

## Building from source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run directly
cargo run --release -- --color ff00ff
```

## Troubleshooting

### Ring doesn't appear
- Make sure your compositor supports `wlr-layer-shell`
- Check if another overlay is blocking it

### Tray icon doesn't show
- Ensure you have a system tray running (e.g., Waybar with tray module)
- Check D-Bus is running: `systemctl --user status dbus`

### High CPU usage
- This is expected during animations
- Use `--animation none` for minimal CPU usage

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

This project is licensed under the **GNU General Public License v3.0 or later** - see the [LICENSE](LICENSE) file for details.

---

<p align="center">
  Made with Rust for the Hyprland community
</p>
