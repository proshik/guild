#!/bin/sh
# quit.sh <pid> — cmd-q, Return on the sheet, then say whether the window and
# the sessions it started are gone.
here=$(cd "$(dirname "$0")" && pwd); T=${WINCTL:-$here/bin/winctl}; P=$1
$T key $P 12 cmd; sleep 1.5; $T key $P 36; sleep 3
ps -p $P >/dev/null && echo "ALIVE $P" || echo "gone $P"
echo "children-by-socket: $(ps eww -ax | grep "guild-hooks-$P.sock" | grep -v grep | wc -l)"
