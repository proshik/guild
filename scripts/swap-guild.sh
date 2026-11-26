#!/bin/sh
# Put a built Guild.app in place of another — M5.13, reshaped in M5.15.
#
#   swap-guild.sh <new bundle> <bundle to replace> [<pid> [<folder>]]
#
# Without a pid it is `just install` with no Guild running: the swap and
# nothing else.
#
# With a pid it is the window's Update button. The window runs this from
# a copy of its own bundle's `Contents/Resources/swap-guild.sh`, in a
# temporary folder, because the bundle it came from is the one replaced;
# it starts it in a session of its own and quits. So this waits for that
# pid to exit, swaps, and opens the new Guild — with `open -n`, so that
# LaunchServices starts this path rather than answering with some other
# Guild of the same bundle id, and with `GUILD_STATE_DIR` passed on when
# it is set, so a Guild started with its own state comes back to it
# (task 0 of M5.15's plan measured both).
#
# The old bundle is moved aside rather than deleted until the new one is
# in place, and put back if the copy fails — and then, with a pid, opened
# again: the person asked for an update, not for no Guild at all.
#
# With a folder, that folder is removed when this exits, however it
# exits — the window names its temporary one, which holds the copy and
# this script, so an update leaves nothing in $TMPDIR. It is named rather
# than worked out from $0: from `scripts/` or a bundle's `Resources/` the
# folder this lives in is not garbage. And it is removed only when it is
# the new bundle's own, absolute and not `/`: a wrong argument costs a
# leftover, never a checkout. Removing the file `sh` is reading is safe —
# the descriptor it holds keeps the text, which the swap_guild tests
# measure — and the trap runs last anyway.
set -eu

staged=$1
app=$2
pid=${3:-}
folder=${4:-}

if [ -n "$folder" ]; then
    case "$folder" in
        /?*)
            if [ "$(dirname "$staged")" = "$folder" ]; then
                trap 'rm -rf "$folder"' EXIT
            else
                echo "swap-guild: $folder does not hold $staged; it is kept" >&2
            fi
            ;;
        *)
            echo "swap-guild: $folder is not an absolute folder; it is kept" >&2
            ;;
    esac
fi

reopen() {
    if [ -n "${GUILD_STATE_DIR:-}" ]; then
        open -n --env "GUILD_STATE_DIR=$GUILD_STATE_DIR" "$1"
    else
        open -n "$1"
    fi
}

if [ -n "$pid" ]; then
    trap '' HUP
    while kill -0 "$pid" 2>/dev/null; do
        sleep 1
    done
fi

old="$app.old"
rm -rf "$old"
if [ -d "$app" ]; then
    mv "$app" "$old"
fi
if ! ditto "$staged" "$app"; then
    rm -rf "$app"
    if [ -d "$old" ]; then
        mv "$old" "$app"
    fi
    echo "swap-guild: the copy failed; the old Guild is back" >&2
    if [ -n "$pid" ] && [ -d "$app" ]; then
        reopen "$app"
    fi
    exit 1
fi
rm -rf "$old" "$staged"
# ditto preserves the staged directory timestamp. Announce this replacement
# to Finder/LaunchServices without clearing caches or restarting the Dock.
touch "$app"
echo "swap-guild: $app replaced"

if [ -n "$pid" ]; then
    reopen "$app"
fi
