# Driving a Guild window for a plan's window checks

What the checks of M5.11 and M5.12 were run with on 2026-09-15. Nothing
here is built by `just`; nothing here is part of the product.

    cd scripts/window-check && mkdir -p bin
    swiftc -O winctl.swift -o bin/winctl
    swiftc -O probe.swift  -o bin/probe
    swiftc -O locked.swift -o bin/locked

- `bin/probe` must print `true` twice — Accessibility and Screen Recording
  for the application the session runs in (the terminal, or the agent's app).
- `bin/locked` prints `locked: 1` while the screen is locked: nothing can
  be driven then, and a checker waiting on it stalls.
- `idle.sh` waits until nobody is using the keyboard and mouse — a
  modifier held by `winctl` combines with the person's own keys.
- `launch.sh` starts a window with an isolated `XDG_CONFIG_HOME` and a
  bare environment; `quit.sh` quits it and counts what it left.
- `DRIVING.md` is the guide a checker is given: every `winctl` command,
  the macOS key codes (the layout may not be US — paste text, never
  type letters by key code), and the rules that kept clicks from landing in
  the wrong window.

Keep the Mac awake for a long run with `caffeinate -dimsu -t 14400`.
