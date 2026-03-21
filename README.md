# Minal

AI-first terminal emulator built with Rust and wgpu.

Minal integrates AI as a first-class feature — command completion, inline chat, and error analysis — on top of a GPU-accelerated terminal with a 3-thread architecture inspired by Ghostty.

> **Status:** Early development (v0.1.0). Core terminal features work; AI features are in progress.

## Features

- **GPU-accelerated rendering** — wgpu (Metal / Vulkan / DX12) at 120fps
- **3-thread architecture** — Main (winit event loop), I/O (tokio + PTY), Renderer (wgpu)
- **AI completion** — Ghost text suggestions via Ollama, Anthropic, or OpenAI
- **Tabs & panes** — Split vertically/horizontally, per-pane PTY
- **Full color** — 256-color, TrueColor, theme presets (Catppuccin Mocha, Tokyo Night, Dracula, Solarized), hot-reload
- **Mouse support** — X10/SGR protocols, text selection, scroll wheel
- **Clipboard** — Copy/paste, OSC 52, auto-copy on select
- **Shell integration** — OSC 133 semantic prompts (bash, zsh, fish)
- **macOS native** — IME, dark mode, menu bar, window attributes
- **TOML configuration** — `~/.config/minal/minal.toml` with sensible defaults

## Requirements

- **Rust** 1.85+ (edition 2024)
- **OS:** macOS or Linux
- **GPU:** Metal (macOS) or Vulkan-capable GPU (Linux)

### Linux dependencies

```bash
# Ubuntu / Debian
sudo apt-get install -y libvulkan-dev mesa-vulkan-drivers

# Fedora
sudo dnf install vulkan-loader-devel mesa-vulkan-drivers
```

macOS requires no additional dependencies.

## Build

```bash
git clone https://github.com/m1ng112/minal.git
cd minal

# Debug build
cargo build

# Release build (recommended for daily use)
cargo build --release
```

The binary is placed at `target/release/minal` (or `target/debug/minal`).

## Run

```bash
# Debug
cargo run

# Release
cargo run --release

# Or run the binary directly
./target/release/minal

# Enable debug logging
RUST_LOG=debug cargo run
```

## Test

```bash
# All tests
cargo test --workspace

# Individual crates
cargo test -p minal-core
cargo test -p minal-renderer
cargo test -p minal-ai
cargo test -p minal-config

# GPU-specific tests (requires GPU)
cargo test -p minal-renderer -- --ignored
```

## Configuration

Minal loads config from `~/.config/minal/minal.toml`. All fields are optional and fall back to defaults.

```toml
[window]
width = 80         # columns
height = 24        # rows
opacity = 1.0
padding = 8

[font]
family = "JetBrains Mono"
size = 14.0

[theme]
preset = "catppuccin-mocha"   # catppuccin-mocha | tokyo-night | dracula | solarized | solarized-light | custom

[ai]
provider = "ollama"           # ollama | anthropic | openai
debounce_ms = 300
ghost_text_opacity = 0.5

[ai.privacy]
send_cwd = true
max_output_chars = 2000
max_command_history = 20

[clipboard]
auto_copy = false
```

### Key bindings (macOS defaults)

| Key | Action |
|-----|--------|
| `Cmd+C` | Copy |
| `Cmd+V` | Paste |
| `Cmd+T` | New tab |
| `Cmd+W` | Close pane/tab |
| `Cmd+Shift+]` | Next tab |
| `Cmd+Shift+[` | Previous tab |
| `Cmd+D` | Split vertical |
| `Cmd+Shift+D` | Split horizontal |
| `Cmd+=` | Increase font size |
| `Cmd+-` | Decrease font size |
| `Tab` | Accept AI completion |
| `Esc` | Dismiss AI completion |

### Shell integration

Source the appropriate script in your shell config:

```bash
# bash (~/.bashrc)
[[ "$TERM_PROGRAM" == "minal" ]] && source "$MINAL_SHELL_INTEGRATION/minal.bash"

# zsh (~/.zshrc)
[[ "$TERM_PROGRAM" == "minal" ]] && source "$MINAL_SHELL_INTEGRATION/minal.zsh"

# fish (~/.config/fish/config.fish)
if test "$TERM_PROGRAM" = "minal"
    source "$MINAL_SHELL_INTEGRATION/minal.fish"
end
```

## Project structure

```
minal/
├── src/                    # Main application (event loop, window, tabs/panes)
├── crates/
│   ├── minal-core/         # Terminal emulation (VT parser, grid, PTY)
│   ├── minal-renderer/     # GPU rendering (wgpu, text/rect pipelines)
│   ├── minal-ai/           # AI engine (providers, completion, context)
│   └── minal-config/       # Configuration (TOML, themes, keybinds)
└── shell-integration/      # Shell scripts for OSC 133 prompts
```

## License

MIT
