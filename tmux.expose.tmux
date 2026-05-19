#!/usr/bin/env bash

set -euo pipefail

key="$(tmux show-option -gqv @tmux-expose-key)"
key_table="$(tmux show-option -gqv @tmux-expose-key-table)"
width="$(tmux show-option -gqv @tmux-expose-width)"
height="$(tmux show-option -gqv @tmux-expose-height)"
command="$(tmux show-option -gqv @tmux-expose-command)"

if [[ -z "${key}" ]]; then
  key="M-e"
  key_table="${key_table:-root}"
else
  key_table="${key_table:-prefix}"
fi

width="${width:-100%}"
height="${height:-100%}"
command="${command:-tmux-expose}"

tmux bind-key -T "${key_table}" "${key}" display-popup -w "${width}" -h "${height}" -E "${command}"
