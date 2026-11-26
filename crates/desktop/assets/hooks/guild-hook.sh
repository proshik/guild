#!/bin/sh
# Guild's hook: hands the hook's JSON to the window that started this
# session, and nothing else. M5.10.
#
# Two harnesses speak through it. Claude posts its own envelope whole on
# stdin — the hook command is just `'<script>'`, and the JSON already
# names the event and the session. Codex (and anything like it) names the
# event on the command line (`'<script>' PreToolUse`) and leaves the
# session's own id in GUILD_SESSION, so the envelope is built here, and
# Codex's *own* id is lifted out of the event JSON it sends on stdin as
# `codex_session`, which is what lets a reopened session resume;
# without the argument, stdin is passed through unchanged.
#
# Every envelope from a process Guild started also names it as
# `guild_session`, from GUILD_SESSION — M13.3. After `/resume` Claude names
# the conversation it resumed, not the tab it runs in; the window needs
# both to make that tab the resumed session's. A process with no
# GUILD_SESSION posts exactly what it posted before.
#
# It never fails the hook and never holds it up: every path ends in
# exit 0, curl gives up after a second, and stdin is read even when there
# is nowhere to send it, so the CLI never writes into a closed pipe.
#
# `Expect:` is cleared because curl otherwise sends `Expect: 100-continue`
# for a body over a kilobyte and waits a second for the answer — the whole
# of --max-time, and a PostToolUse carries the tool's whole output.
post() {
  if [ -z "${GUILD_HOOK_SOCKET:-}" ] || [ ! -S "$GUILD_HOOK_SOCKET" ]; then
    cat >/dev/null
    return
  fi
  # A launch nonce cannot turn an old process's delayed hook into the new
  # runtime's delivery acknowledgement. Legacy launches retain their envelope.
  {
    if [ -n "${GUILD_RUNTIME:-}" ]; then
      /usr/bin/plutil -replace guild_runtime -string "$GUILD_RUNTIME" -o - -
    else
      /bin/cat
    fi
  } | /usr/bin/curl -fsS -X POST --unix-socket "$GUILD_HOOK_SOCKET" \
    --connect-timeout 0.5 --max-time 1 \
    -H 'Content-Type: application/json' -H 'Expect:' \
    --data-binary @- http://guild/hook >/dev/null 2>&1
}
if [ -n "${1:-}" ]; then
  # Codex hands the event's JSON to the hook's stdin, and it names the
  # harness's own session id — which Guild does not assign as it does
  # Claude's, Codex generates its own. Lifting that id out of it is what
  # lets a reopened project resume the real conversation, the same way
  # opencode's plugin sends `oc_session`. Reading stdin through to the
  # end is also what keeps the write from blocking the agent: the
  # `exec 0<&-` that preceded this was closing a pipe nothing read.
  # Parse the top-level key: tool payloads can contain another session_id.
  codex_payload=$(/bin/cat)
  codex_payload=$(printf '%s' "$codex_payload" | /usr/bin/plutil -convert json -o - - 2>/dev/null) || codex_payload='{"hook_event_name":""}'
  # Empty JSON dictionaries are ambiguous to plutil's replacement writer;
  # seed a routing key, while retaining JSON nulls in all real tool payloads.
  [ "$codex_payload" = '{}' ] && codex_payload='{"hook_event_name":""}'
  codex_session=$(printf '%s' "$codex_payload" | /usr/bin/plutil -extract session_id raw -o - - 2>/dev/null) || codex_session=""
  # Native subagents inherit the parent's Guild environment. Their hook
  # agent_id, or a guardian's object-valued session_meta source, is an
  # authoritative subprocess witness: neither may notify or persist identity.
  codex_agent=$(printf '%s' "$codex_payload" | /usr/bin/plutil -extract agent_id raw -o - - 2>/dev/null) || codex_agent=""
  [ -n "$codex_agent" ] && exit 0
  codex_transcript=$(printf '%s' "$codex_payload" | /usr/bin/plutil -extract transcript_path raw -o - - 2>/dev/null) || codex_transcript=""
  if [ -f "$codex_transcript" ]; then
    codex_meta=$(/usr/bin/head -n 1 "$codex_transcript")
    codex_meta_id=$(printf '%s' "$codex_meta" | /usr/bin/plutil -extract payload.id raw -o - - 2>/dev/null) || codex_meta_id=""
    if [ "$codex_meta_id" = "$codex_session" ]; then
      codex_subagent=$(printf '%s' "$codex_meta" | /usr/bin/plutil -extract payload.source.subagent json -o - - 2>/dev/null) || codex_subagent=""
      [ -n "$codex_subagent" ] && exit 0
    fi
  fi
  # Persist before notification: a stopped window cannot acknowledge identity.
  # Separate immutable candidates prevent delayed events from selecting an old
  # conversation after an in-TUI resume. The reader refuses ambiguity.
  uuid='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  if [ -n "${GUILD_IDENTITY_DIR:-}" ] &&
     [ "${#GUILD_SESSION}" -eq 36 ] && [ "${#codex_session}" -eq 36 ] &&
     printf '%s\n' "${GUILD_SESSION:-}" | /usr/bin/grep -Eq "$uuid" &&
     printf '%s\n' "$codex_session" | /usr/bin/grep -Eq "$uuid"; then
    (
      umask 077
      directory="$GUILD_IDENTITY_DIR/$GUILD_SESSION"
      /bin/mkdir -p "$directory" || exit 0
      target="$directory/$codex_session.json"
      [ -f "$target" ] && exit 0
      draft=$(/usr/bin/mktemp "$directory/.writing.XXXXXX") || exit 0
      trap '/bin/rm -f "$draft"' EXIT
      printf '{"guild_session":"%s","codex_session":"%s"}\n' \
        "$GUILD_SESSION" "$codex_session" >"$draft" && /bin/mv -f "$draft" "$target"
    ) 2>/dev/null
  fi
  # Keep the actual request/tool identities and input; unrelated parallel
  # completions must not resolve an outstanding approval. Override only routing.
  printf '%s' "$codex_payload" |
    /usr/bin/plutil -replace hook_event_name -string "$1" -o - - |
    /usr/bin/plutil -replace session_id -string "$GUILD_SESSION" -o - - |
    /usr/bin/plutil -replace codex_session -string "$codex_session" -o - - |
    /usr/bin/plutil -replace guild_session -string "$GUILD_SESSION" -o - - |
    /usr/bin/plutil -convert json -o - - | post
elif [ -n "${GUILD_SESSION:-}" ]; then
  # Claude writes its envelope as one object on one line, and a session id
  # is a UUID, so the field goes in after the opening brace with nothing
  # to escape.
  /usr/bin/sed "1s/^{/{\"guild_session\":\"$GUILD_SESSION\",/" | post
else
  post
fi
exit 0
