#!/bin/sh
# launch.sh <cfg-dir> <cwd> <logfile> [extra env...] — start an isolated Guild
# window and print its `winctl list` line (pid, window id, frame).
here=$(cd "$(dirname "$0")" && pwd); T=${WINCTL:-$here/bin/winctl}
repo=$(cd "$here/../.." && pwd)
cfg=$1; cwd=$2; log=$3; shift 3
before=$($T list guild | awk '{print $1}')
cd "$cwd" && env -i HOME=$HOME USER=$USER SHELL=/bin/zsh LANG=en_US.UTF-8 PATH=$HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin XDG_CONFIG_HOME="$cfg" "$@" "$repo/target/debug/guild-desktop" > "$log" 2>&1 &
sleep 7
$T list guild | while read line; do
  pid=${line%% *}; echo "$before" | /usr/bin/grep -qx "$pid" || echo "$line"
done
