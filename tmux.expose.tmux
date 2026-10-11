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
attention_color="$(tmux show-option -gqv @tmux-expose-attention-color)"
waiting_color="$(tmux show-option -gqv @tmux-expose-waiting-color)"
working_color="$(tmux show-option -gqv @tmux-expose-working-color)"
agent_sort="$(tmux show-option -gqv @tmux-expose-agent-sort)"
vim_keys="$(tmux show-option -gqv @tmux-expose-vim-keys)"
command="$(tmux show-option -gqv @tmux-expose-command)"
next_key="$(tmux show-option -gqv @tmux-expose-next-key)"
next_key_table="$(tmux show-option -gqv @tmux-expose-next-key-table)"
next_binary="$(tmux show-option -gqv @tmux-expose-binary)"

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
next_key_table="${next_key_table:-prefix}"

# `next` is a headless subcommand of the same binary, so take just the
# program from @tmux-expose-command by default. This is a plain split on the
# first space -- never `eval`, which would execute anything else in that
# string ($(...), ;, &&, a pipeline) as a side effect of plugin startup. The
# tradeoff is that it can't handle a quoted executable path that itself
# contains a space (e.g. '/opt/my tools/tmux-expose' --columns 2); set
# @tmux-expose-binary explicitly in that case rather than relying on this to
# parse it out.
next_binary="${next_binary:-${command%% *}}"

# run-shell isn't attached to a client, so pass the pressing client's session
# and name explicitly. #{q:...} asks tmux to shell-quote the expanded value
# itself, so a session/client name containing quotes or shell metacharacters
# can't break out of the generated command. next_binary is shell-escaped for
# the same reason -- @tmux-expose-binary may itself contain a space.
next_command="$(printf '%q' "${next_binary}") next #{q:session_id} #{q:client_name}"

# Shell-escape color values before splicing them into the -E command string.
# tmux runs that string through the shell, where an unquoted hex value such as
# "#ff8700" would otherwise be swallowed as a comment.
if [[ -n "${selected_color}" ]]; then
  command="${command} --selected-color $(printf '%q' "${selected_color}")"
fi

if [[ -n "${attached_color}" ]]; then
  command="${command} --attached-color $(printf '%q' "${attached_color}")"
fi

if [[ -n "${inactive_color}" ]]; then
  command="${command} --inactive-color $(printf '%q' "${inactive_color}")"
fi

if [[ -n "${attention_color}" ]]; then
  command="${command} --attention-color $(printf '%q' "${attention_color}")"
fi

if [[ -n "${waiting_color}" ]]; then
  command="${command} --waiting-color $(printf '%q' "${waiting_color}")"
fi

if [[ -n "${working_color}" ]]; then
  command="${command} --working-color $(printf '%q' "${working_color}")"
fi

case "$(printf '%s' "${vim_keys}" | tr '[:upper:]' '[:lower:]')" in
  on|true|1|yes) command="${command} --vim" ;;
esac

# Agent-status sorting defaults on; @tmux-expose-agent-sort off/false/0/no opts out.
case "$(printf '%s' "${agent_sort}" | tr '[:upper:]' '[:lower:]')" in
  off|false|0|no) command="${command} --no-agent-sort" ;;
esac

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

# Unbound unless @tmux-expose-next-key is set, so upgrading never takes over a key.
if [[ -n "${next_key}" ]]; then
  tmux bind-key -T "${next_key_table}" "${next_key}" run-shell "${next_command}"
fi
