# Guild's development commands.
#
# The toolchain comes from rust-toolchain.toml — through rustup, not through
# Homebrew. `just` here is deliberately thin: it assembles compound commands
# rather than renaming one-liners. Everything a recipe does is visible right
# in it.
#
# The two details the recipes exist for:
#   check  — the order fmt → lint → test is mandatory: fmt changes files that
#            clippy then reads, and the reverse order gives a false "clean".

# Show the list of recipes.
default:
    @just --list --unsorted

# The full check before a commit: fmt, lint, boundaries, test.
check: fmt lint boundaries test

# The boundaries the compiler does not check.
boundaries:
    @bash scripts/check-boundaries.sh

# Format.
fmt:
    cargo fmt --all

# Check the formatting without changing files (for CI).
fmt-check:
    cargo fmt --all --check

# Lint, warnings = errors.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Build the workspace with a status row per crate.
#
# One cargo, not one per crate: cargo locks the target directory, so a
# process per row would queue up and the table would show a parallelism
# that is not happening. The rows are drawn from cargo's own stream —
# `Compiling` is a start, an artifact is a finish — and cargo compiles
# crates in parallel by itself, which is the parallelism you can see.
build-all *ARGS:
    python3 scripts/build.py build --workspace {{ARGS}}

# The same picture for the desktop alone.
build-desktop *ARGS:
    python3 scripts/build.py build -p guild-desktop {{ARGS}}

# The quick loop, for the minutes between tasks.
#
# It was worth much more while the server shared this tree: it skipped
# four crates and a database. The workspace now holds the desktop, session,
# release and CLI crates, so what this saves is the boundaries guard
# and nothing else — which is still a `cargo tree` per crate, and still
# worth having between tasks.
#
# **It is not a substitute for `check` before a commit:** it says nothing
# about the boundaries, and those are the rules the compiler cannot see.
quick:
    cargo fmt --all
    cargo clippy --all-targets -- -D warnings
    cargo test -- --test-threads=4

# All tests. Four concurrent tests bound native processes and filesystem contention;
# per-test deadlines remain real assertions, not load-dependent startup benchmarks.
test:
    cargo test --workspace -- --test-threads=4

# Headless CI on supported Unix runners; no window or installed model CLI needed.
test-headless:
    cargo test -p guild-session -p guild-cli --locked -- --test-threads=4

# Opt-in tests for local transcript discovery and isolated resume persistence.
# No provider is launched; local transcript discovery needs existing user data.
test-live:
    cargo test -p guild-session -- --ignored --nocapture

# The tests of one crate, optionally filtered: just test-crate guild-desktop --lib FILTER
# Desktop's public CLI fixtures require the CURRENT guild binary in this target.
# Bare cargo regression commands must first run cargo build -p guild-cli --bin guild
# with the same target/profile (or explicitly supply GUILD_TEST_CLI for isolated tests).
test-crate crate *ARGS:
    if [ "{{crate}}" = guild-desktop ]; then cargo build -p guild-cli --bin guild; fi
    cargo test -p {{crate}} {{ARGS}}

# A quick compilation check.
build:
    cargo check --workspace

# Open the Guild window. DEVELOPER_DIR is set in .cargo/config.toml.
# Explicit native helper preparation, never part of Cargo/check. Vendored JS only.
prepare-diagrams:
    python3 tools/diagrams/renderer/build.py

desktop: prepare-diagrams
    GUILD_DIAGRAM_RESOURCES="{{justfile_directory()}}/target/diagram-runtime" cargo run -p guild-desktop

# Build Guild.app.
#
# Without a bundle this is a binary rather than an application: no icon, no
# name in the menu bar, no identity in the Dock or Cmd-Tab, and no
# notifications.
#
# The .icns is assembled by the system's sips and iconutil from
# assets/icon.png so that building needs no Python or SVG renderer.
# assets/icon.py renders the committed PNG from the editable assets/icon.svg.
bundle: prepare-diagrams
    cargo build --release -p guild-desktop -p guild-cli --locked
    bash scripts/bundle.sh target/release/Guild.app target/release target/diagram-runtime

# Build and open Guild.app.
app: bundle
    open target/release/Guild.app

# A dev build: a branch being tried, in the Guild you work in. Refuses a
# tree with uncommitted changes to tracked files, since what is installed
# must be a commit that can be named and gone back to — and refuses while
# the installed Guild runs, **first**, before the check and the tag: a dev
# build is never offered to the Update button (M5.16), so there is nothing
# to stage it for. Quit Guild, or release `main`. docs/desktop-dev.md.
#
# Install a dev build of the Guild you work in.
install dest="/Applications":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "$(just _running "{{dest}}")" = running ]; then
        echo "refused: the installed Guild is running — a dev build is never offered to its Update button." >&2
        echo "Quit Guild and run just install again, or release main with just release." >&2
        exit 1
    fi
    just _installable
    scripts/swap-guild.sh target/release/Guild.app "{{dest}}/Guild.app"

# Only from a clean `main`, and only with something under `## [Unreleased]`
# in CHANGELOG.md — both asked before five minutes of `just check`. Then:
# the name (`nightly-<date>`, `-<short hash>` for a second that day), the
# changelog cut, a commit of CHANGELOG.md alone, the tag, the bundle; and
# staged for the Update button while the installed Guild runs, swapped in
# otherwise. **Never pushed** — the push is printed for the person. M5.16,
# decision 45; docs/desktop-dev.md.
#
# Release main as a nightly, and hand it to the Guild you work in.
release dest="/Applications":
    #!/usr/bin/env bash
    set -euo pipefail
    branch=$(git symbolic-ref --short -q HEAD || true)
    if [ "$branch" != main ]; then
        echo "refused: a release is made from main — this is ${branch:-a detached HEAD}" >&2
        exit 1
    fi
    if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
        echo "refused: tracked files have uncommitted changes — release a commit" >&2
        exit 1
    fi
    # The changelog's door: guild_session::changelog, the module the
    # window reads with. No gpui in it, so it builds in seconds.
    cargo build -q -p guild-cli --bin guild-release
    tool=target/debug/guild-release
    "$tool" keys-check assets/update-keys.json
    "$tool" check CHANGELOG.md
    echo "just check…"
    just check > target/release-check.log 2>&1 || {
        echo "refused: just check failed — target/release-check.log" >&2
        tail -20 target/release-check.log >&2
        exit 1
    }
    # fmt writes files, and the release commits CHANGELOG.md alone.
    if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
        echo "refused: just check changed tracked files — commit them, then release" >&2
        git status --short --untracked-files=no >&2
        exit 1
    fi
    date=$(date +%F)
    name=$(git tag -l 'nightly-*' | "$tool" name "$date" "$(git rev-parse --short HEAD)")
    "$tool" cut CHANGELOG.md "$name" "$date"
    git add CHANGELOG.md
    git commit -q -m "Release $name"
    git tag "$name"
    echo "$(git rev-parse --short HEAD) is $name"
    # From here the release is true — committed and tagged — whatever
    # the bundle does, so a failure says how to finish, not how to undo.
    GUILD_PRODUCTION_BUNDLE=1 RELEASE_TAG="$name" just bundle || {
        echo "the release $name is committed and tagged, and the bundle failed —" >&2
        echo "finish with: just bundle && just _stage target/release/Guild.app" >&2
        exit 1
    }
    if [ "$(just _running "{{dest}}")" = running ]; then
        just _stage target/release/Guild.app
    else
        scripts/swap-guild.sh target/release/Guild.app "{{dest}}/Guild.app"
    fi
    echo "not pushed — to publish: git push origin main $name"

# "Running" is asked of `ps`, not `pgrep`: macOS's pgrep leaves out its
# own ancestors, and a terminal tab of Guild is one of Guild's
# descendants — so from inside Guild pgrep never saw it, and the bundle
# was swapped under the running window. `comm` is a process's argv[0],
# which LaunchServices sets to the executable's full path, and it is
# compared as a whole line: an argument that mentions the path, as this
# shell's own does, is not a match. Not `grep -q`: it stops at the first
# match, and under pipefail the SIGPIPE `ps` then dies of would read
# as "not running". docs/traps.md. It prints rather than fails, so a
# caller's `if` has no `just` error on its stderr.
#
# Whether the Guild installed in `dest` is running: `running` or `not running`.
_running dest:
    #!/usr/bin/env bash
    set -euo pipefail
    if ps -axo comm= | grep -Fx "{{dest}}/Guild.app/Contents/MacOS/guild-desktop" >/dev/null; then
        echo running
    else
        echo "not running"
    fi

# The state folder by the rule of `desktop::paths::state_dir`, in this
# shell's environment: a Guild started with another GUILD_STATE_DIR looks
# elsewhere, which is why the folder is printed. The copy is made beside
# and renamed in, so the window never reads half a bundle as a build.
#
# Stage a completed bundle using the same publication lock for every producer.
_stage bundle destination="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "${GUILD_STATE_DIR:-}" ]; then
        state=$GUILD_STATE_DIR
    elif [ -n "${XDG_CONFIG_HOME:-}" ]; then
        state=$XDG_CONFIG_HOME/guild
    else
        state=$HOME/.config/guild
    fi
    update="{{destination}}"
    if [ -z "$update" ]; then update="$state/update"; fi
    target/release/guild-release stage "{{bundle}}" "$update"
    echo "staged: $update/Guild.app — select Local in Guild's Updates settings"

# Explicit local development loop. Ordinary bundle never stages or publishes.
build-stage destination="": check bundle
    just _stage target/release/Guild.app "{{destination}}"

# What install asks of a tree: clean, a green check, a tag, a bundle.
_installable:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
        echo "refused: tracked files have uncommitted changes — install a commit" >&2
        exit 1
    fi
    echo "just check…"
    just check > target/install-check.log 2>&1 || {
        echo "refused: just check failed — target/install-check.log" >&2
        tail -20 target/install-check.log >&2
        exit 1
    }
    if git tag --points-at HEAD | grep -q '^dogfood-'; then
        tag=$(git tag --points-at HEAD | grep '^dogfood-' | head -1)
    else
        tag="dogfood-$(date +%F)"
        n=2
        while git rev-parse -q --verify "refs/tags/$tag" >/dev/null; do
            tag="dogfood-$(date +%F)-$n"
            n=$((n + 1))
        done
        git tag "$tag"
    fi
    # Tagged before the bundle, which writes the exact tag into itself.
    RELEASE_TAG="$tag" just bundle
    echo "$(git rev-parse --short HEAD) is $tag"

# Its state is kept apart by GUILD_STATE_DIR, and it is started in `dir`,
# which becomes its first project. That may not be this checkout: a
# project's own state is `<root>/.guild/state.json`, and two windows on one
# root write each other's tabs away. Use a worktree or a fixture.
#
# The Guild being developed, beside the one being worked in.
desktop-dev dir state=(env_var("HOME") / ".config/guild-dev"):
    #!/usr/bin/env bash
    set -euo pipefail
    dir=$(cd "{{dir}}" && pwd -P)
    if [ "$dir" = "$(cd "{{justfile_directory()}}" && pwd -P)" ]; then
        echo "refused: the Guild you work in has this checkout open — use a worktree or a fixture" >&2
        exit 1
    fi
    just prepare-diagrams
    cargo build -p guild-desktop -p guild-cli
    cd "$dir"
    GUILD_DIAGRAM_RESOURCES="{{justfile_directory()}}/target/diagram-runtime" GUILD_STATE_DIR="{{state}}" exec "{{justfile_directory()}}/target/debug/guild-desktop"

# The window in an optimized build.
#
# The debug build is several times slower on output: 18 MB/s against 155 MB/s
# on the same file. For working in the terminal the difference is visible to
# the eye, so speed should be judged here rather than in `just desktop`.
desktop-release: prepare-diagrams
    GUILD_DIAGRAM_RESOURCES="{{justfile_directory()}}/target/diagram-runtime" cargo run --release -p guild-desktop










# Check that the active toolchain is the one from rust-toolchain.toml, not brew's.
toolchain:
    @rustup show active-toolchain
    @which cargo
