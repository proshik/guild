# The landing site

`site/` is Guild's landing page: one page built with [Hugo](https://gohugo.io)
and the [Hextra](https://github.com/imfing/hextra) theme. It is not part of
the Cargo workspace and nothing in it is Rust.

## Building it

```sh
./tools/get-hugo.sh    # once: Hugo 0.170.1 into tools/hugo, checked against its SHA-256
just site-serve        # http://localhost:1313, rebuilt on every change
just site              # site/public, with --panicOnWarning
HUGO_BASEURL=https://proshik.github.io/guild/ just site   # built for a sub-path
```

Hextra is a Hugo module pinned in `site/go.mod` (v0.13.0), and Hugo fetches it
with the `go` command, so Go must be installed. Node.js is not needed. The Hugo
binary and the build output are git-ignored. A new Hugo version is a new
`VERSION` and three new checksums in `tools/get-hugo.sh`, taken from the
release's `hugo_<version>_checksums.txt`.

## What is where

- `site/content/_index.md` is the page. Its sections are shortcodes under
  `site/layouts/_shortcodes/landing/`: the hero, the `guild` command line with
  its real commands beside a clip, the five phases as a rail of rows beside a
  stage, a catalog of what else is built in by area, the case for Guild as a
  bento of cards, the questions as an open interview, and the download. The
  page leads with what sets Guild apart: one window for many agents, driven
  from the window or from `guild`.
- `site/assets/css/custom.css` is the page's own styling on top of Hextra, which
  keeps the navbar, the theme switch and the code blocks. Its colours are the
  window's Light and Dark palettes (`crates/desktop/src/theme/palettes.rs`), its
  one accent is the window's brass, and it lays sections out as the window lays
  out its areas: cards six apart on the backdrop, rows like the column's. Its
  first lines are the Hallmark stamp and the waivers sloplint reads.
- `site/layouts/_partials/custom/footer.html` is the footer, a colophon.
- The page is in English at `/` and in Russian at `/ru/`; the navbar's
  language switch moves between them. `site/content/_index.ru.md` is the
  Russian page, section for section the English one, and a change to either is
  made to both. The words that live in the templates (buttons, table heads, the
  colophon) are in `site/i18n/en.yaml` and `ru.yaml`. The Russian page names
  the window's places as Guild's Russian interface does
  (`crates/desktop/src/i18n/russian.rs`); `site/static/og-ru.png` is its
  preview.
- The faces and the icon are not copied: module mounts in `site/hugo.yaml`
  serve Inter and JetBrains Mono from `crates/desktop/assets/fonts` and
  `assets/icon.svg` as the logo and favicon.

**Every claim on the page comes from the README, the [guide](guide.md), the changelog or the code.**
No figure, quote or logo is made up, and no other product is named. A change of
behaviour that makes a sentence untrue is a change to the page too.

## Pictures

- `site/assets/images/<name>-light.webp` and `-dark.webp` are screenshots of the
  window, one per theme; the page shows the one that matches the visitor's
  theme. `sessions` is the hero; `tile-start` and `close` are crops of the window
  for the bento's lead card and the download.
- `site/static/video/<name>-light|dark.{mp4,webm,jpg}` are the clips of the five
  phases and of the command line, H.264 and VP9 without sound, with the last
  frame as the poster. A clip loads only when its phase is chosen.
- `site/static/og.png` is the 1200×630 link preview: the wordmark, the headline
  and the dark hero; redo it when the headline changes.

All of them were taken from an isolated development window (`just desktop-dev`
with its own state directory) on a throwaway demo project under
`/Users/Shared/guild-demo`, with a neutral shell prompt and a demo author, and
with real Claude Code and Codex sessions on small tasks. Nothing personal may
be in a frame: no account, email, plan usage, personal path or real project.
The usage chip is turned off in that window's settings; an agent's own status
line is replaced through the demo worktree's `.claude/settings.local.json`;
and a recording is checked frame by frame for other applications' windows,
which screen capture records when they pass over the window.

## Checks

- `just site` must pass: `--panicOnWarning` turns any Hugo warning into a
  failure.
- Build for a sub-path and load it in a browser once: no request may 404.
- Hallmark's sloplint runs on `site/assets/css/custom.css` against the served
  page, from a directory outside the repository so its `.hallmark/` does not
  land here; the target is no FAIL.

## Publishing

`.github/workflows/pages.yml` builds the site and deploys it to GitHub Pages.
It runs on a push to main that touches what the build reads — the paths are
listed, with the reason for each, at the top of the workflow — and by hand from
the Actions tab or with `gh workflow run pages.yml`. Pages is on, with Settings →
Pages → Source: GitHub Actions.
