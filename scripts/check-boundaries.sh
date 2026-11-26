#!/usr/bin/env bash
# Dependency discovery and matching are separate failures. Never hide cargo errors
# or use grep -q on a live cargo pipe: SIGPIPE under pipefail can invert the result.
set -euo pipefail
check() {
    local crate=$1 forbidden=$2 tree
    if ! tree=$(cargo tree -p "$crate"); then
        echo "boundary check failed: cannot inspect $crate dependencies" >&2
        exit 1
    fi
    if grep -iE "$forbidden" <<<"$tree" >/dev/null; then
        echo "boundary broken: forbidden dependency in $crate" >&2
        grep -iE "$forbidden" <<<"$tree" >&2
        exit 1
    fi
}
check guild-session gpui
check guild-cli gpui
check guild-desktop 'bezel-gpui|gpui-terminal'
echo "boundaries hold: no gpui in session or cli, no gpui fork in the desktop"
