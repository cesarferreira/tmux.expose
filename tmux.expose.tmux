#!/usr/bin/env bash

set -euo pipefail

key="$(tmux show-option -gqv @tmux-expose-key)"
key_table="$(tmux show-option -gqv @tmux-expose-key-table)"
width="$(tmux show-option -gqv @tmux-expose-width)"
height="$(tmux show-option -gqv @tmux-expose-height)"
anchor="$(tmux show-option -gqv @tmux-expose-anchor)"
style="$(tmux show-option -gqv @tmux-expose-style)"
border_style="$(tmux show-option -gqv @tmux-expose-border-style)"
selected_color="$(tmux show-option -gqv @tmux-expose-selected-color)"
attached_color="$(tmux show-option -gqv @tmux-expose-attached-color)"
inactive_color="$(tmux show-option -gqv @tmux-expose-inactive-color)"
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

if [[ -n "${selected_color}" ]]; then
  command="${command} --selected-color ${selected_color}"
fi

if [[ -n "${attached_color}" ]]; then
  command="${command} --attached-color ${attached_color}"
fi

if [[ -n "${inactive_color}" ]]; then
  command="${command} --inactive-color ${inactive_color}"
fi

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
