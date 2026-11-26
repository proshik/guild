# Driving a Guild window for the plan's window checks

Tools: build them as README.md says, into scripts/window-check/bin//
- `winctl list guild`                       → `pid windowid x y w h owner | title` (points, top-left origin)
- `winctl front <pid>`                       activate (via System Events) and print the frontmost pid
- `winctl click <pid> <x> <y> [left|right|double|triple|cmd|shift]`   window-relative POINTS
- `winctl drag <pid> x1 y1 x2 y2`, `winctl move <pid> x y`, `winctl scroll <pid> x y <lines>`
- `winctl swipe <pid> x y dx [steps] [--momentum N]`  precise horizontal swipe with AppKit phases
- `winctl key <pid> <keycode> [cmd] [shift] [alt] [ctrl]`
- `winctl tap <pid> shift 2`                 lone modifier taps (double Shift)
- `winctl paste <pid> "<text>"`              pbcopy + cmd-v (the keyboard layout may not be US: never type letters by keycode, paste text)
- `winctl shot <pid> <file.png>`             window capture; the PNG is 2x (retina): divide pixel coordinates by 2 to get points
- `probe`                                    AXIsProcessTrusted / screen capture rights (both must be true)
Every gesture activates the pid and refuses if it is not frontmost. A click holds 150 ms.

macOS key codes: a0 s1 d2 f3 h4 g5 z6 x7 c8 v9 b11 q12 w13 e14 r15 y16 t17 1=18 2=19 3=20 4=21 6=22 5=23 =24 9=25 7=26 -27 8=28 0=29 ]30 o31 u32 [33 i34 p35 return36 l37 j38 k40 ;41 \42 ,43 /44 n45 m46 .47 tab48 space49 `50 delete51 esc53 f5=96 f6=97 f7=98 f3=99 f8=100 f9=101 f11=103 f12=111 home115 pageup116 fwddelete117 end119 f2=120 pagedown121 f1=122 left123 right124 down125 up126

Launching an isolated window (never the person's Guild.app, never their config):
  cd / && env -i HOME=$HOME USER=$USER SHELL=/bin/zsh LANG=en_US.UTF-8 PATH=/usr/bin:/bin:/usr/sbin:/sbin \
     XDG_CONFIG_HOME=<fixture>/cfg <checkout>/target/debug/guild-desktop > <fixture>/out.log 2>&1 &
  then `winctl list guild` for the pid (never trust $!). Start from `/` so no stray project is added.
  Add a project with Recent/Open or by writing the workspace file beforehand if the plan says how; otherwise use cmd-o and the folder chooser (cmd-shift-g, paste path, return).
  Wait ~5 s after launch before the first gesture; send the first key twice if nothing happens (the first key after activation can be eaten).
Quit with cmd-q (key 12) so state is flushed; check `ps` that the pid and its children are gone (`pgrep -P <pid>`); a SIGTERM leaves claude children — kill those too.

Rules:
- Before believing "the click does nothing": re-capture, check the pid is frontmost, retry once with a longer pause.
- Read what the check says must be SEEN or MEASURED (a screenshot's content, a file's bytes, git output, a process list) and record that evidence.
- Other apps may grab the front; winctl refuses — just retry the gesture.
