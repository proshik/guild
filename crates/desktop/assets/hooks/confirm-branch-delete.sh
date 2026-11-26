#!/bin/sh
# This process belongs to one git branch -d. Ref locks are held in prepared.
# Preserve the configured reference-transaction hook and its exact input/status.
set -eu
cat > "$GUILD_BRANCH_DELETE_INPUT"
if [ "$1" = prepared ]; then
    count=0
    while read -r old new reference; do
        [ "$reference" = "$GUILD_BRANCH_DELETE_REFERENCE" ] || exit 1
        case "$new" in ''|*[!0]*) exit 1 ;; esac
        count=$((count + 1))
    done < "$GUILD_BRANCH_DELETE_INPUT"
    [ "$count" = 1 ] || exit 1
    if git symbolic-ref --quiet "$GUILD_BRANCH_DELETE_REFERENCE" >/dev/null 2>&1; then
        echo 'Confirmed branch became symbolic; deletion refused.' >&2
        exit 1
    fi
    actual=$(git rev-parse --verify --end-of-options "$GUILD_BRANCH_DELETE_REFERENCE")
    if [ "$actual" != "$GUILD_BRANCH_DELETE_COMMIT" ]; then
        echo 'Confirmed branch commit changed; deletion refused.' >&2
        exit 1
    fi
fi
if [ -x "$GUILD_BRANCH_DELETE_HOOK" ]; then
    "$GUILD_BRANCH_DELETE_HOOK" "$@" < "$GUILD_BRANCH_DELETE_INPUT"
fi
if [ "$1" = committed ]; then
    : > "$GUILD_BRANCH_DELETE_MARKER"
fi
