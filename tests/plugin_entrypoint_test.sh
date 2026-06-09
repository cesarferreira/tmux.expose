#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

run_plugin() {
  local tmpdir
  tmpdir="$(mktemp -d)"

  cat >"${tmpdir}/tmux" <<'FAKE_TMUX'
#!/usr/bin/env bash
set -euo pipefail

if [[ "$1" == "show-option" ]]; then
  option_name="${@: -1}"
  case "${option_name}" in
    @tmux-expose-key) printf '%s' "${TMUX_EXPOSE_TEST_KEY:-}" ;;
    @tmux-expose-key-table) printf '%s' "${TMUX_EXPOSE_TEST_KEY_TABLE:-}" ;;
    @tmux-expose-width) printf '%s' "${TMUX_EXPOSE_TEST_WIDTH:-}" ;;
    @tmux-expose-height) printf '%s' "${TMUX_EXPOSE_TEST_HEIGHT:-}" ;;
    @tmux-expose-anchor) printf '%s' "${TMUX_EXPOSE_TEST_ANCHOR:-}" ;;
    @tmux-expose-style) printf '%s' "${TMUX_EXPOSE_TEST_STYLE:-}" ;;
    @tmux-expose-border-style) printf '%s' "${TMUX_EXPOSE_TEST_BORDER_STYLE:-}" ;;
    @tmux-expose-selected-color) printf '%s' "${TMUX_EXPOSE_TEST_SELECTED_COLOR:-}" ;;
    @tmux-expose-attached-color) printf '%s' "${TMUX_EXPOSE_TEST_ATTACHED_COLOR:-}" ;;
    @tmux-expose-inactive-color) printf '%s' "${TMUX_EXPOSE_TEST_INACTIVE_COLOR:-}" ;;
    @tmux-expose-command) printf '%s' "${TMUX_EXPOSE_TEST_COMMAND:-}" ;;
  esac
  exit 0
fi

printf '%q ' "$@" >"${TMUX_EXPOSE_TEST_OUTPUT}"
FAKE_TMUX
  chmod +x "${tmpdir}/tmux"

  TMUX_EXPOSE_TEST_OUTPUT="${tmpdir}/output" \
    TMUX_EXPOSE_TEST_KEY="${TMUX_EXPOSE_TEST_KEY:-}" \
    TMUX_EXPOSE_TEST_KEY_TABLE="${TMUX_EXPOSE_TEST_KEY_TABLE:-}" \
    TMUX_EXPOSE_TEST_WIDTH="${TMUX_EXPOSE_TEST_WIDTH:-}" \
    TMUX_EXPOSE_TEST_HEIGHT="${TMUX_EXPOSE_TEST_HEIGHT:-}" \
    TMUX_EXPOSE_TEST_ANCHOR="${TMUX_EXPOSE_TEST_ANCHOR:-}" \
    TMUX_EXPOSE_TEST_STYLE="${TMUX_EXPOSE_TEST_STYLE:-}" \
    TMUX_EXPOSE_TEST_BORDER_STYLE="${TMUX_EXPOSE_TEST_BORDER_STYLE:-}" \
    TMUX_EXPOSE_TEST_SELECTED_COLOR="${TMUX_EXPOSE_TEST_SELECTED_COLOR:-}" \
    TMUX_EXPOSE_TEST_ATTACHED_COLOR="${TMUX_EXPOSE_TEST_ATTACHED_COLOR:-}" \
    TMUX_EXPOSE_TEST_INACTIVE_COLOR="${TMUX_EXPOSE_TEST_INACTIVE_COLOR:-}" \
    TMUX_EXPOSE_TEST_COMMAND="${TMUX_EXPOSE_TEST_COMMAND:-}" \
    PATH="${tmpdir}:${PATH}" \
    bash "${repo_root}/tmux.expose.tmux"
  tr -d '\n' <"${tmpdir}/output"
}

assert_equals() {
  local expected="$1"
  local actual="$2"

  if [[ "${actual}" != "${expected}" ]]; then
    printf 'Expected:\n%s\n\nActual:\n%s\n' "${expected}" "${actual}" >&2
    exit 1
  fi
}

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 100% -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(run_plugin)"

assert_equals \
  'bind-key -T prefix E display-popup -w 100% -h 100% -e TMUX_EXPOSE_TOGGLE_KEY=E -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_KEY=E run_plugin)"

assert_equals \
  'bind-key -T root C-e display-popup -w 80% -h 70% -e TMUX_EXPOSE_TOGGLE_KEY=C-e -E tmux-expose\ --columns\ 2 ' \
  "$(TMUX_EXPOSE_TEST_KEY=C-e TMUX_EXPOSE_TEST_KEY_TABLE=root TMUX_EXPOSE_TEST_WIDTH=80% TMUX_EXPOSE_TEST_HEIGHT=70% TMUX_EXPOSE_TEST_COMMAND='tmux-expose --columns 2' run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 50% -y \#\{popup_pane_bottom\} -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_ANCHOR=bottom TMUX_EXPOSE_TEST_HEIGHT=50% run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 50% -y \#\{popup_pane_top\} -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_ANCHOR=top TMUX_EXPOSE_TEST_HEIGHT=50% run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 50% -h 100% -x \#\{popup_pane_right\} -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_ANCHOR=right TMUX_EXPOSE_TEST_WIDTH=50% run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 50% -h 100% -x \#\{popup_pane_left\} -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_ANCHOR=left TMUX_EXPOSE_TEST_WIDTH=50% run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 100% -s bg=colour234 -S fg=colour245 -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_STYLE='bg=colour234' TMUX_EXPOSE_TEST_BORDER_STYLE='fg=colour245' run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 100% -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose\ --selected-color\ magenta ' \
  "$(TMUX_EXPOSE_TEST_SELECTED_COLOR=magenta run_plugin)"

assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 100% -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose\ --selected-color\ magenta\ --attached-color\ blue\ --inactive-color\ colour245 ' \
  "$(TMUX_EXPOSE_TEST_SELECTED_COLOR=magenta TMUX_EXPOSE_TEST_ATTACHED_COLOR=blue TMUX_EXPOSE_TEST_INACTIVE_COLOR=colour245 run_plugin)"

# Hex values must be shell-escaped so the popup command isn't truncated at '#'.
assert_equals \
  'bind-key -T root M-e display-popup -w 100% -h 100% -e TMUX_EXPOSE_TOGGLE_KEY=M-e -E tmux-expose\ --selected-color\ \\\#ff8700 ' \
  "$(TMUX_EXPOSE_TEST_SELECTED_COLOR='#ff8700' run_plugin)"
