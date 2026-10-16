#!/usr/bin/env bash
# SPDX-FileCopyrightText: The flint contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

# Claude Code `Stop` hook: runs `just lint-fast` when the agent tries to
# finish its turn, and if it fails, sends the output back so the agent
# fixes it before stopping. Wired up in .claude/settings.json.
#
# Exit 0 lets the agent stop; exit 2 blocks it, with stderr as the reason.
set -uo pipefail

# How many times in a row the hook blocks before letting the agent stop
# anyway, so a check it can't fix (e.g. a missing tool) can't loop forever.
max_attempts=3

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

if ! command -v just >/dev/null 2>&1; then
    echo "stop-lint: just isn't installed, so skipping lint-fast" >&2
    exit 0
fi

# The hook's input says whether this stop follows one the hook already
# blocked; if not, it's a fresh stop and the count starts over.
input="$(cat)"
counter=target/claude-stop-hook-attempts
mkdir -p target
if [[ "$input" =~ \"stop_hook_active\":[[:space:]]*true ]]; then
    attempts="$(cat "$counter" 2>/dev/null || echo 0)"
else
    attempts=0
fi

# Through mise when it's installed, so the checks run at the versions
# mise.toml pins even though this shell may not have mise activated.
run=(just lint-fast)
if command -v mise >/dev/null 2>&1; then
    run=(mise exec -- just lint-fast)
fi

if output="$("${run[@]}" 2>&1)"; then
    rm -f "$counter"
    exit 0
fi

attempts=$((attempts + 1))
echo "$attempts" >"$counter"
if ((attempts > max_attempts)); then
    rm -f "$counter"
    echo "stop-lint: just lint-fast still fails after $max_attempts attempts; letting the turn end" >&2
    exit 0
fi

{
    echo "just lint-fast fails — fix it before finishing (see AGENTS.md, \"Linting is not optional\"):"
    echo
    # The tail is where the errors and just's own summary are.
    printf '%s\n' "$output" | tail -n 80
} >&2
exit 2
