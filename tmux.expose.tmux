#!/usr/bin/env bash

set -euo pipefail

key="$(tmux show-option -gqv @tmux-expose-key)"
width="$(tmux show-option -gqv @tmux-expose-width)"
height="$(tmux show-option -gqv @tmux-expose-height)"
command="$(tmux show-option -gqv @tmux-expose-command)"

key="${key:-E}"
width="${width:-100%}"
height="${height:-100%}"
command="${command:-tmux-expose}"

tmux bind-key "${key}" display-popup -w "${width}" -h "${height}" -E "${command}"
