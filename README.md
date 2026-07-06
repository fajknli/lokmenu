# lok

A CJK-optimized Wayland menu tool. Supports fuzzy matching, pinyin input, and fractional scaling.

## Dependencies

Requires a Wayland compositor with `wlr-layer-shell`, `text-input-v3`, and optionally `fractional-scale` / `viewporter` support.

## Install

```bash
cargo build --release
```

## Usage

```bash
echo -e "firefox\nchromium\nalacritty\nthunar" | lok -p "Open: "
```

## Parameters

| Flag | Long | Description | Default |
|------|------|-------------|---------|
| `-p` | `--prompt` | Prompt prefix text | (empty) |
| `-i` | `--output-index` | Output selected index instead of content | off |
| `-n` | `--lines` | Visible rows (0 = auto, max 20) | 0 |
| `-W` | `--width` | Window width in pixels (0 = fullscreen) | 0 |
| `-s` | `--font-size` | Font size | 14.0 |
| `-f` | `--font` | Font name | (system default) |
| `-m` | `--multi-select` | Multi-select mode (Tab to mark, Enter to confirm) | off |
| `-0` | `--null` | NUL-separate multi-select output (for `xargs -0`) | off |
| `-P` | `--password` | Password mode (hide input, no list) | off |
| | `--anchor` | Window position: `top` or `bottom` | top |
| `-b` | `--bg` | Background color | `#141522` |
| | `--fg` | Normal text color | `#A9B5D5` |
| | `--sbg` | Selected item background | `#565D7E` |
| | `--sfg` | Selected item text | `#D4DCF2` |
| | `--hfg` | Match highlight color | `#C93B3B` |
| | `--prompt-bg` | Input box background | `#1B1D2B` |
| | `--prompt-fg` | Input box text color | `#A9B5D5` |

## Examples

```bash
# Application launcher
ls /usr/bin | lok -p "Run: " | xargs swaymsg exec --

# Power menu
echo -e "shutdown\nreboot\nlock\nsuspend" | lok -p "Power: " | xargs systemctl

# Clipboard history
cliphist list | lok -p "Paste: " | cliphist decode | wl-copy

# Multi-select files
find . -type f | lok -m -p "Files: " | xargs -d '\n' cat

# Multi-select with NUL separator
find . -type f | lok -m -0 -p "Files: " | xargs -0 stat

# Password input
lok -P -p "Password: "

# Bottom-anchored menu
ls | lok -p "Select: " --anchor bottom

# Fixed width
ls | lok -p "Select: " -W 400

# Output index
echo -e "first\nsecond\nthird" | lok -i -p "Pick: "
```

## Keybindings

| Key | Action |
|-----|--------|
| Type | Filter list (fuzzy match + pinyin) |
| Up / Ctrl+P | Move selection up |
| Down / Ctrl+N | Move selection down |
| Enter | Confirm selection |
| Esc / Ctrl+C | Cancel |
| Backspace | Delete character |
| Ctrl+U | Clear input |
| Ctrl+W | Delete word |
| Tab | Toggle mark (multi-select mode) |
| Mouse Left | Select item |
| Mouse Right | Toggle mark (multi-select mode) |
| Scroll | Navigate list |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Selection made |
| 1 | User cancelled (Esc) |
| 3 | Error (e.g. no Wayland display) |

## CJK Support

- Fuzzy match on original text (works for any language)
- Pinyin full-match (e.g. type `beijing` to match `北京`)
- Pinyin initials match (e.g. type `bj` to match `北京`)
- Multi-pronunciation support (e.g. `行` matches both `hang` and `xing`)
