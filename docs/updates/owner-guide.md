# Update sources and deliberate distribution

This is an owner runbook, not execution authorization. The owner
authorized initial key setup on 2026-10-18; the public verification
map now contains owner-2026. Pushes, CI execution, Release publication, repository
visibility changes, installation and external canvas publication have not been
performed. Key creation does not authorize those actions.

## Running Guild

Settings → Updates offers **Remote** (default) and **Local**, with **Nightly**
(default) and **Stable**. The first Remote provider is `proshik/guild` on GitHub
Releases. An empty Stable channel never falls back to Nightly. Channel selection
is retained in Local mode. Preferences live in `update-preferences.json` beside
Guild's state, independently of legacy `settings.json`; an interrupted save asks
which exact pending selection to retain before checking/installing.

The recipient needs the app and macOS arm64, without a checkout, Rust, Xcode or
command-line developer tools. Public releases use anonymous HTTPS; gh is optional
and unused for successful public requests. For the currently private repository,
the same owner installs gh on the recipient Mac and explicitly authorizes it:

```bash
gh auth login --hostname github.com
gh api repos/proshik/guild --silent
```

Guild never starts interactive login, extracts a token, or changes gh settings.
Private/missing/inaccessible responses can be ambiguous; check authorized repo
access manually if necessary. A stale gh credential does not prevent anonymous
access after the repository becomes public. Changing visibility needs no updater
replacement, key change or migration build. Live private/public/visibility-change
qualification remains pending; fixture tests are not evidence of those operations.

Download verifies owner signatures, digest, archive layout, architecture, app ID,
minimum OS and code seal before exposing a ready candidate. Install and restart
confirms the exact artifact, including rebuild/reinstall/rollback and source
replacement. Build time is informational; rebuilding a commit is not a newer
product version. Source/channel changes revoke old downloads and confirmations.

## Local development

Select Local and optionally choose an absolute staging directory. Empty uses
`<Guild state>/update`, normally `~/.config/guild/update` (or the configured
`GUILD_STATE_DIR` / `XDG_CONFIG_HOME` equivalent).

```bash
just build-stage '/absolute/path/completed-builds'
```

This explicitly runs checks, builds and stages a complete bundle and completion
marker. Untagged modern development builds are eligible. `just bundle` assembles
without staging, installing, uploading or publishing. To stage an already built
bundle, use `just _stage target/release/Guild.app '/absolute/path/completed-builds'`.
Never copy a half-built app into the watched directory or manufacture the ready
marker. Producers serialize on the canonical persistent staging lock. A newer
producer can repair its owned incomplete publication without modifying the
installed app. Both Macs run their app locally; no server/runtime forwarding is
involved.

## Publisher trust bootstrap

Choose an owner-controlled Ed25519 **raw 32-byte seed** and stable key ID. The
seed file must be a nonsymlink regular file, owned by the current user with mode
0600. Keep it outside the checkout, artifacts and CI. Do not put its contents in
arguments, shell history, logs, environment variables or a GitHub secret.

If the owner authorizes a new key, a separately chosen protected directory and
exclusive file creation are required. For example, in an owner terminal only:

```bash
publisher_key='/owner-selected/protected-directory/guild-update.seed'
( set -C; umask 077; head -c 32 /dev/urandom > "$publisher_key" )
```

Do not rerun this to replace an existing key. Initial setup is already complete:
owner-2026 uses ~/.config/guild/update-signing/owner-2026.seed, with directory mode
0700 and seed mode 0600. Creation was exclusive; only the public map was exported
to the repository. No production release signatures have been issued. Back up
this existing seed through the owner's approved secret storage; the seed itself
is never committed or copied to CI/the recipient Mac. Public map export prints
only the public verification key:

```bash
cargo build -p guild-cli --bin guild-release
target/debug/guild-release public-key "$publisher_key" owner-2026 > /owner-selected/new-public-map.json
```

Inspect/merge the public map into `assets/update-keys.json`, preserving existing
keys during rotation. Commit that public-only change **before** choosing the
release revision. `guild-release keys-check assets/update-keys.json` rejects an
empty map; CI packaging requires a configured key. The initial public key
owner-2026 is now configured on this branch; commit it
before choosing the first CI revision. A release-supplied key is never a trust
bootstrap.

For rotation, first ship an old-key-signed app containing both public keys, then
sign later releases with the new key. A lost/compromised sole key needs an
explicitly authenticated manual reinstall; do not bypass signature verification.

## Build an exact tested revision on GitHub

For the explicit end-to-end Nightly procedure, invoke the project
[/release skill](../../.claude/skills/release/SKILL.md). That invocation authorizes
its tag push, owner-local signing, verified prerelease publication and local
build/staging, without separate phase approvals. Completion includes both the
published verified GitHub prerelease and a ready Local update of the same checked
tagged commit. The skill uses the active Guild state's selected `local_directory`
(or `<active state>/update`, normally `~/.config/guild/update`), honoring runtime
state overrides without changing source or preferences. Remote selection does not
skip Local staging or switch to Local. A bundle/ZIP in `target` is insufficient:
the staged app and completion marker must match the verified local build identity.
A repeated invocation resumes the matching release and completes only missing
work; it does not cut another tag for a missing Local stage. Installation and
restart remain the user's action. The manual steps below require their own
explicit authorization when performed outside that skill invocation.

After the branch is reviewed and integrated through separately authorized actions:

1. Choose an existing `nightly-*` or exact `vMAJOR.MINOR.PATCH` tag on main's
   first-parent history, with its dated CHANGELOG section. Ensure the public key
   is already included in that exact revision. Test **that tagged revision**
   locally with `just check`; a release-cut commit changes the revision and needs
   its own verification. Keep its full commit ID and check log.
2. Pushing a `nightly-*` or `v*` tag automatically runs the bundle workflow.
   For a separately authorized manual build, run it from main with the exact tag:
   `gh workflow run bundle.yml --repo proshik/guild --ref main -f tag="$tag"`.
3. Select the exact successful run/attempt and download its named artifact to a
   fresh directory: `gh run download "$run_id" --repo proshik/guild --name
   "Guild-${tag}-macos-arm64" --dir "$artifact_dir"`. Actions retains it seven
   days. Confirm workflow run/attempt, full commit, selected tag, arm64 and build
   identity against local evidence. The artifact contains ZIP, SHA256,
   build-info.json/txt and unsigned guild-update.json; it is not a Release.
4. On the owner Mac, sign the **unchanged downloaded bytes**, using fresh output:

```bash
target/debug/guild-release sign \
  "$artifact_dir/guild-update.json" "$artifact_dir/$archive" \
  "$publisher_key" owner-2026 "$signed_dir"
target/debug/guild-release verify "$signed_dir" assets/update-keys.json
```

The signer checks digest, embedded build/native identity and the selected public
key inside that exact CI app. It refuses a mismatched key or existing output.
Do not rebuild/repack/resign the app after this step. Keep the unsigned CI
artifact, signed manifest/archive signatures and verification log together.

5. After separate publication authorization, create/review a draft Release for
   the existing tag and upload these four required assets from `$signed_dir`:
   `guild-update.json`, `guild-update.json.sig`, the exact archive basename from
   the manifest, and `<archive>.sig`. Additional build-info/SHA256 files are
   useful owner evidence. Nightly must be a prerelease; Stable must not be a
   prerelease. Drafts are never discoverable as updates. Publishing the verified
   draft is a separate external action, not a side effect of signing.
6. Qualify private access on the other Mac; later qualify public anonymous access
   with no gh/token/checkout and stale saved gh credentials. Repository visibility
   changes and actual publication require their own explicit authorization.

## First installation and legacy updater bootstrap

The payload is a ZIP containing Guild.app. Extract the verified exact archive
with macOS resource-preserving tooling, then explicitly choose/install the app.
DMG, Developer ID and notarization are deferred; the current ad-hoc app may need
macOS approval. This branch has not installed anything in /Applications.

The old installed updater only accepts a **later-built Nightly** in its fixed
state/update location. It cannot bootstrap from Remote, Stable, an untagged dev
build or a custom staging directory. Before using that old button:

- Record the exact old app/commit/build time/state directory and bootstrap identity.
- Explicitly choose a separate external backup path, copy the working old app
  there with `ditto`, verify its code seal and reopen that backup in an isolated
  state if authorized. Never call this backup `app.old`: the old helper deletes
  its own app.old before opening the new app.
- Verify the bootstrap app and legacy offer conditions before staging it to the
  old fixed location. Do not forge timestamps. Stage/backup do not authorize an
  /Applications replacement. Unsupported bootstrap cases need separately
  authorized manual installation.
- If first launch fails, quit any running affected app and restore the verified
  external backup manually. The old helper has no new acknowledgement guarantee.
  Do not use the new journal commands for a transaction the old helper never wrote.

After that bootstrap, new updates retain a unique previous app until the exact
new instance acknowledges its first main frame. Native modern-to-modern fixture
observations do not establish legacy bootstrap acceptance; that live scenario
remains pending.

## Interrupted new-updater recovery

Settings displays a pending transaction and a copyable, shell-quoted recovery
command. A successful acknowledgement retires only its own backup. Missing ack
retains the previous app; it does not kill a possibly live new instance or
silently restore over it. Journals are under `<state>/update/transactions`.

Quit all affected Guild copies and keep them closed for the full operation.
Run the command from Settings, or use the known retained previous bundle if the
new app cannot start:

```bash
'/exact/retained/Guild.app/Contents/MacOS/guild-desktop' \
  --guild-update-recover '/exact/Guild-state' TRANSACTION_UUID \
  restore-previous --apps-closed
```

The command works without gh, a checkout or Rust. It requires a native-verified
executable belonging to that exact transaction, checks the owner's live executable
paths, and uses the same anchored journal/path/proof validation and destination
lock. It refuses foreign/changed/symlink/unexpected files instead of deleting
those paths. The process check supplements the owner's closed-app assertion;
keep the app closed to avoid a new-launch race. It signals no process and does
not automatically launch the recovered app. Reopen the recorded destination
explicitly after success, retaining the same GUILD_STATE_DIR if customized.

An explicit `keep-current` choice is also supported by the private command when
the owner has verified the current app and wants to retire that transaction's
backup. This permanently discards that backup; it is not a workaround for a
failed proof or mismatched state. Never edit a receipt or remove an unfamiliar
backup to make a check pass.

## Update card

Settings → Updates presents the installed version, channel and current status in
one card. Its main action follows the existing state: Check, Download, Install
and restart, or Cancel while uncommitted work is active. Release notes expand
inline; category buttons count only actual Keep a Changelog entries. Additional
settings and details is an independent disclosure for Remote/Local, the local
staging directory, build provenance, available release history and deliberate
replacement/reinstallation. Preference and transaction recovery controls remain
visible when required. Restart confirmation names the exact replacement and
explains unsaved-file checks, session termination and retained conversations;
it does not schedule a restart or promise unfinished work will complete.

Presentation can be checked without a provider or installed app using
`cargo run -p guild-desktop --features native-preview --example update-card-preview`.
Set an absolute task-owned `GUILD_STATE_DIR`, `GUILD_NO_GLOBAL_HOOKS=1`, and create
`.owned-update-card-fixture` there containing `Guild isolated update card fixture`
followed by a newline. Optional `GUILD_UPDATE_CARD_STATE` selects `available`
(default), `ready`, `current`, `checking`, `downloading`, `empty` or `error`.
Metadata/notes are illustrative. The Diagnostic root has no provider or bundle
destination: actions cannot install/restart an app. Use only disposable settings.
