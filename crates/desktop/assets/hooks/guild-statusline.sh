#!/bin/sh
# Guild's status line: taps the plan's limits Claude Code streams to a
# status line, hands them to the window that started this session, then
# runs the person's own status line with the same input — so theirs is
# what shows. Guild names this script only for a person who has a status
# line of their own (GUILD_USER_STATUSLINE); nobody gets a row they did
# not have.
#
# Claude runs a status line several times a second while it streams, so a
# tick that does not post does only builtins and one `date`: the input is
# read with `read`, the stamp with `read`, and the post — one per session
# per fifteen seconds, only of a payload that carries `rate_limits` — is
# a backgrounded subshell whose stdin and stdout are not the status
# line's. Claude reads the status line from this script's stdout until it
# closes; a post holding it would hold the person's status bar up.
#
# `Expect:` is cleared for guild-hook.sh's reason: curl otherwise waits a
# second before a body over a kilobyte.
PAYLOAD=
separator=
while IFS= read -r line || [ -n "$line" ]; do
  PAYLOAD="${PAYLOAD}${separator}${line}"
  separator='
'
done

if [ -n "${GUILD_HOOK_SOCKET:-}" ] && [ -S "$GUILD_HOOK_SOCKET" ]; then
  case "$PAYLOAD" in
    *'"rate_limits"'*)
      # A session id is a UUID; anything else shares one stamp rather
      # than be put into a path.
      case "${GUILD_SESSION:-}" in
        '' | *[!A-Za-z0-9_-]*) key=default ;;
        *) key=$GUILD_SESSION ;;
      esac
      stamp="${0%/*}/statusline/$key"
      now=$(date +%s)
      last=0
      if [ -f "$stamp" ]; then
        read -r last <"$stamp" 2>/dev/null
      fi
      case "$last" in
        '' | *[!0-9]*) last=0 ;;
      esac
      if [ $((now - last)) -ge 15 ]; then
        printf '%s' "$now" >"$stamp" 2>/dev/null
        (
          printf '{"provider":"claude","guild_session":"%s","payload":%s}' "$key" "$PAYLOAD" |
            "${0%/*}/guild-usage-post.sh" "$GUILD_HOOK_SOCKET"
        ) </dev/null >/dev/null 2>&1 &
      fi
      ;;
  esac
fi

if [ -n "${GUILD_USER_STATUSLINE:-}" ]; then
  printf '%s\n' "$PAYLOAD" | sh -c "$GUILD_USER_STATUSLINE"
fi
