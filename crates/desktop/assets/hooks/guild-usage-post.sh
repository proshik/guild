#!/bin/sh
# The statusline's HTTP adapter. Its caller owns backgrounding and descriptor
# isolation; this command reads the complete JSON envelope from stdin.
exec /usr/bin/curl -fsS -X POST --unix-socket "$1" \
  --connect-timeout 0.5 --max-time 1 \
  -H 'Content-Type: application/json' -H 'Expect:' \
  --data-binary @- http://guild/usage
