# tmux-expose

`tmux-expose` is a Rust terminal UI for switching tmux sessions from a Mission Control-style grid of live text previews.

## Install

From this repository:

```bash
cargo install --path .
```

## Usage

```bash
tmux-expose
```

Refresh interval defaults to 500ms:

```bash
tmux-expose --refresh-interval 500
```

Thumbnail sizing can be adjusted with the minimum thumbnail width and a forced column count:

```bash
tmux-expose --thumbnail-width 48 --columns 2
```

## tmux Binding

Add this to `~/.tmux.conf`:

```tmux
bind-key E display-popup -w 95% -h 90% -E "tmux-expose"
```

Reload tmux config, then press your tmux prefix followed by `E`.

## Controls

```text
Arrow keys / hjkl  Move selection
Enter              Switch to selected session
q / Esc / Ctrl-C   Quit without switching
```

## macOS Gesture Integration

Use BetterTouchTool, Hammerspoon, Raycast, or another automation tool to trigger:

```bash
tmux display-popup -w 95% -h 90% -E "tmux-expose"
```

The app itself is terminal-only and does not depend on macOS-specific APIs.
