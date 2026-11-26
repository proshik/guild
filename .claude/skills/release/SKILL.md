---
name: release
description: "Build, sign locally and publish a Guild Nightly only on explicit /release or $release invocation. Not for release discussion or skill editing."
disable-model-invocation: true
---

# Release a Nightly

Explicit invocation authorizes tag, push, local signing, publication and local
build/staging steps without further phase approvals. Honor narrower instructions
and host permissions.
Never release as a development test. Completion requires both a verified published
GitHub prerelease and a completed Local update in the selected staging directory.
Do not install or restart, change global settings or update preferences, transfer
the signing seed to CI, or invoke legacy `just release`.
Read [the owner guide](../../../docs/updates/owner-guide.md) for signing requirements.

Before cutting a new release, inspect evidence of an interrupted invocation and
any matching published release, CI artifact and Local ready identity. Resume the
same exact tag/commit/run/release when identities match; do not choose a new tag
or cut/push another release merely because Local staging is missing. Revalidate
existing remote assets as in steps 5–9. For this resume path, use a clean checkout
of the already checked tagged commit on remote main's first-parent history,
retaining the final `just check` evidence; do not require an old release commit
to equal a subsequently advanced remote main. Stop on ambiguity or mismatch.

1. Use a clean `main` checkout of `proshik/guild`. Fetch origin and require HEAD
   to equal `origin/main`; report the full SHA and any excluded local work.
   If the caller has changes or another branch, use a fresh clone of remote main
   outside it; preserve caller files and do not merge task branches automatically.
   In a fresh clone, carry and check the caller's effective Git author/committer
   identity only; never invent an identity or copy all Git configuration.
   Require the tag-triggered `.github/workflows/bundle.yml` on remote main before
   cutting. Check the effective Git author/committer identity in this checkout.
2. Build the existing CLI with `cargo build --locked -p guild-cli --bin guild-release`.
   Use its actual binary path (respect `CARGO_TARGET_DIR`) as `$release_cli` below.
   Choose a fresh external directory for logs and downloaded artifacts.
   Run `"$release_cli" check CHANGELOG.md` and
   `"$release_cli" keys-check assets/update-keys.json`.
3. Set `$date` to today's `YYYY-MM-DD`; choose `$tag` with
   `git tag -l 'nightly-*' | "$release_cli" name "$date" "$(git rev-parse --short HEAD)" CHANGELOG.md`.
   Run `"$release_cli" cut CHANGELOG.md "$tag" "$date"`, inspect the changelog,
   then `git add -- CHANGELOG.md` and
   `git commit --only -m "Release $tag" -- CHANGELOG.md`.
   Record `$commit` with `git rev-parse HEAD`. Run `just check` on this final revision;
   require a clean checkout afterward. Stop on failure; never tag unchecked changes.
4. Create a lightweight tag with `git tag --no-sign "$tag" "$commit"`.
   Confirm origin has one matching fetch/push destination and remote main has not
   moved from the recorded base. Push exactly these refs, without force:
   `git push --atomic --no-follow-tags origin "${commit}:refs/heads/main" "refs/tags/${tag}:refs/tags/${tag}"`.
5. Find the `bundle.yml` push run with this exact full commit and tag using
   `gh run list --repo proshik/guild --workflow bundle.yml --event push --commit "$commit"`.
   Inspect its run ID/attempt, head SHA, tag and conclusion with `gh api`; require
   one unambiguous successful match. Poll for at most ten minutes, then report the
   pending run/link for resumption. Never guess the latest run or dispatch/rerun CI.
6. Identify the single artifact named `Guild-${tag}-macos-arm64` on that run; record
   its ID and attempt, then `gh run download "$run_id" --repo proshik/guild
   --name "Guild-${tag}-macos-arm64" --dir "$artifact_dir"` into a fresh directory.
   Check manifest and build-info against repository, tag, full commit, run/attempt,
   arm64, clean source and GitHub provenance; check the archive SHA256/size.
   Preserve these bytes unchanged: no rebuilding, repacking or native resigning.
7. Use the owner's existing key (default `~/.config/guild/update-signing/owner-2026.seed`,
   ID `owner-2026`), never print or copy it. With the archive name from the manifest:
   `"$release_cli" sign "$artifact_dir/guild-update.json" "$artifact_dir/$archive" "$publisher_key" owner-2026 "$signed_dir"`.
   Use fresh output and run `"$release_cli" verify "$signed_dir" assets/update-keys.json`.
   The existing signer checks native identity and the key embedded in the app.
8. Save manifest release notes to `$notes_file`. Inspect any existing release first;
   otherwise `gh release create "$tag" --repo proshik/guild --verify-tag --draft
   --prerelease --title "$tag" --notes-file "$notes_file"`.
   Upload with `gh release upload`: signed manifest, manifest signature, archive,
   archive signature, plus original build-info.json/txt and SHA256 evidence.
   Never use `--clobber`. Record the release ID; download its uploaded assets by
   that ID into a fresh directory, compare every hash, and run `guild-release verify`
   there against `assets/update-keys.json`. A failed check must never publish.
9. Publish the verified draft with `gh release edit "$tag" --repo proshik/guild
   --draft=false --prerelease`; verify public state and report its URL, tag, commit
   and CI run/attempt. On interruption inspect and resume the same tag/run/release:
   upload only missing assets after verifying existing ones. Stop on ambiguity or
   mismatch; never move tags or overwrite releases. An already-published matching
   release is verified success, not something to rewrite. Retain logs on failure.
10. Prepare the Local update in this same invocation, including when the active
    source is Remote. Resolve the owning running Guild's actual state directory
    and read its `update-preferences.json`: use its selected absolute
    `local_directory`, or `<active state>/update` when empty. Runtime state may be
    overridden by `GUILD_STATE_DIR`, then `XDG_CONFIG_HOME/guild`; the normal
    fallback is `~/.config/guild/update`. Confirm the active runtime location
    rather than guessing from a fresh shell or clone when an override is in use.
    An unresolved state path or pending preference selection is a real dependency;
    report it instead of silently staging to a default or changing preferences.
    Record the resolved destination as `$local_directory`. Never switch Remote
    to Local automatically; staging is required independently of source selection.
11. Reuse a completed matching Local build when its source and artifact identities
    are already verified. Otherwise, in the clean checkout of the same checked
    tagged `$commit`, require `git rev-parse "$tag^{commit}"` and HEAD to equal
    `$commit`, then run `GUILD_PRODUCTION_BUNDLE=1 RELEASE_TAG="$tag" just bundle`.
    Use the recipe's normal checkout-local `target` layout (unset an inherited
    `CARGO_TARGET_DIR` for this recipe, whose copy paths are fixed). Do not set
    GitHub CI provenance variables on a local build. This is a separate Local
    artifact; the downloaded/signed/uploaded CI archive remains unchanged.
    Require `codesign --verify --deep --strict target/release/Guild.app` and
    `lipo -archs target/release/Guild.app/Contents/MacOS/guild-desktop` to confirm
    exactly arm64. Read `Contents/Resources/build-info.json` and compatibility
    `commit`, `tag`, `built` resources: require exact `$commit` and `$tag`, clean
    source, local provenance and matching resources; record the `build_id`.
    Stop if the code seal or identity fails even if `just bundle` exited zero.
12. Stage the verified bundle through the existing bounded producer:
    `just _stage target/release/Guild.app "$local_directory"` (equivalently
    `target/release/guild-release stage target/release/Guild.app "$local_directory"`).
    Do not copy bundles or manufacture completion markers. Verify the staged app's
    code seal and embedded identity again, and require
    `$local_directory/guild-local-ready.json` schema 1, `commit` and `build_id`
    to match both the staged app and the verified local source build exactly.
    On resume, skip build/stage when this completed source/artifact identity
    already matches; if only staging is missing, stage the existing verified
    bundle without rebuilding or repeating publication. A `.app` or ZIP under
    `target` alone is never a prepared Local update. Report the public release
    URL/CI identity and Local destination/commit/build ID/ready verification.
    Leave installation and restart to the user; do not claim a visible offer
    was observed unless it was actually checked in the running Local source.
