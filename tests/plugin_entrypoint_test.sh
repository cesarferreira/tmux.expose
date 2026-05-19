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
  'bind-key -T root M-e display-popup -w 100% -h 100% -E tmux-expose ' \
  "$(run_plugin)"

assert_equals \
  'bind-key -T prefix E display-popup -w 100% -h 100% -E tmux-expose ' \
  "$(TMUX_EXPOSE_TEST_KEY=E run_plugin)"

assert_equals \
  'bind-key -T root C-e display-popup -w 80% -h 70% -E tmux-expose\ --columns\ 2 ' \
  "$(TMUX_EXPOSE_TEST_KEY=C-e TMUX_EXPOSE_TEST_KEY_TABLE=root TMUX_EXPOSE_TEST_WIDTH=80% TMUX_EXPOSE_TEST_HEIGHT=70% TMUX_EXPOSE_TEST_COMMAND='tmux-expose --columns 2' run_plugin)"
