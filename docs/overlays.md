# Guild overlays

An overlay is a presentation owned by a session or native controller. It covers
one session or pane, not the whole window. Its lifetime is independent of the
operation it observes. External programs use the generic terminal channel.

## From a harness or shell session

Use the `guild` executable and socket/session environment supplied by Guild:

```sh
guild overlay open build --title "Build output" --from-file build.log --json
guild overlay hide build --json
guild overlay show build --json
guild overlay close build --json
guild overlay list --json
```

Opening the same ID updates its title and text. An update preserves whether the
person minimized it. Showing a surface hides any other expanded surface on the
same target and content channel; split siblings are unaffected. CLI commands cannot manipulate a
surface belonging to another session. Content is literal text, never HTML,
JavaScript or a shell command. The CLI does not start or stop a producer.

The compact control reopens the surface. Clicking it focuses the overlay;
Escape or Minimize returns focus and keeps the compact control. Close removes
the presentation. Producer-specific cancellation is a separate explicit action.
Background content updates never take keyboard focus or select another tab.

Presentation records are currently window-local, capped at 64 surfaces with
1 MiB of text each. Producers keep their own durable data. Existing retired native
review archives remain untouched; Guild no longer reads or executes their state.

## From native Guild features

`guild_session::overlay::Board` is the headless state model. A `Target::Session`
follows a session; `Target::Pane` addresses an arbitrary native pane. Use stable
IDs and an explicit owner. `Content` is a typed renderer payload; add native
renderers without introducing operation lifecycles into the board. The common
GPUI host owns frame, visibility, scrolling and focus. Each surface has its own
focus handle, including surfaces simultaneously visible in split panes.

The text and generic terminal renderers share this host. A new build, test,
search or task view reuses it without introducing a second lifecycle owner.
Terminal Jobs retains process receipts and optional archive_viewer metadata;
Open run invokes the registered external viewer with literal archive arguments.
Native review rendering, configuration and cancellation are retired.
