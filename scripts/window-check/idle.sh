#!/bin/sh
# idle.sh [ms]: wait until no HID input for ms (default 2500), at most 60 s
need=${1:-2500}; n=0
while :; do
  i=$(ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print int($NF/1000000); exit}')
  [ "$i" -ge "$need" ] && exit 0
  n=$((n+1)); [ $n -gt 240 ] && { echo "never idle"; exit 1; }
  sleep 0.25
done
