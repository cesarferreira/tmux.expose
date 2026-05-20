#!/usr/bin/env bash

set -euo pipefail

key="$(tmux show-option -gqv @tmux-expose-key)"
key_table="$(tmux show-option -gqv @tmux-expose-key-table)"
width="$(tmux show-option -gqv @tmux-expose-width)"
height="$(tmux show-option -gqv @tmux-expose-height)"
anchor="$(tmux show-option -gqv @tmux-expose-anchor)"
style="$(tmux show-option -gqv @tmux-expose-style)"
border_style="$(tmux show-option -gqv @tmux-expose-border-style)"
command="$(tmux show-option -gqv @tmux-expose-command)"

if [[ -z "${key}" ]]; then
  key="M-e"
  key_table="${key_table:-root}"
else
  key_table="${key_table:-prefix}"
fi

width="${width:-100%}"
height="${height:-100%}"
anchor="${anchor:-center}"
command="${command:-tmux-expose}"

position_args=()
case "${anchor}" in
  center) ;;
  top) position_args=(-y '#{popup_pane_top}') ;;
  bottom) position_args=(-y '#{popup_pane_bottom}') ;;
  left) position_args=(-x '#{popup_pane_left}') ;;
  right) position_args=(-x '#{popup_pane_right}') ;;
  *)
    printf 'tmux.expose: invalid @tmux-expose-anchor: %s\n' "${anchor}" >&2
    exit 1
    ;;
esac

style_args=()
if [[ -n "${style}" ]]; then
  style_args+=(-s "${style}")
fi

if [[ -n "${border_style}" ]]; then
  style_args+=(-S "${border_style}")
fi

tmux bind-key -T "${key_table}" "${key}" display-popup -w "${width}" -h "${height}" "${position_args[@]}" "${style_args[@]}" -e "TMUX_EXPOSE_TOGGLE_KEY=${key}" -E "${command}"
