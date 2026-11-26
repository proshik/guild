#!/usr/bin/env bash
# Record identity before codesigning. Never tags, builds, installs or publishes.
set -euo pipefail
resources=$1
tool=$2
commit=$(git rev-parse HEAD)
tag=$(git tag --points-at HEAD | "$tool" select-tag "${RELEASE_TAG:--}")
cleanliness=clean
if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
    cleanliness=dirty
fi
# Optional reproducible assembly epoch; one value still feeds both identity formats.
built=${SOURCE_DATE_EPOCH:-$(date -u +%s)}
order=-
if [[ "$tag" == nightly-* || "$tag" == v* ]]; then
    order=$(git rev-list --first-parent --count HEAD)
fi
repository=-
run=-
attempt=-
if [ "${GITHUB_ACTIONS:-false}" = true ]; then
    [[ -n "${RELEASE_TAG:-}" && "$tag" != - && "$cleanliness" == clean ]]
    # Merge ancestry alone allows a side-branch commit; product order is main's chain.
    git rev-list --first-parent origin/main | grep -Fx "$commit" >/dev/null
    repository=${GITHUB_REPOSITORY:?}
    run=${GITHUB_RUN_ID:?}
    attempt=${GITHUB_RUN_ATTEMPT:?}
fi
"$tool" build-info "$resources/build-info.json" "$commit" "$tag" "$order" \
    "$cleanliness" "$built" "$repository" "$run" "$attempt"
# The legacy updater and the new metadata must describe exactly the same bytes.
{
    echo "$commit"
    if [ "$cleanliness" = dirty ]; then echo dirty; fi
} > "$resources/commit"
echo "$built" > "$resources/built"
if [ "$tag" != - ]; then echo "$tag" > "$resources/tag"; fi
