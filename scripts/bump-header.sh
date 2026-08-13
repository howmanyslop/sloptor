#!/usr/bin/env bash
# Keeps emitted `-- Compiled with sloptor v...` header lines in golden files
# and Go test expectations in sync with internal/version/version.go.
#
# Usage:
#   ./scripts/bump-header.sh             # update all files
#   ./scripts/bump-header.sh --check     # exit non-zero if any file is stale

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "bump-header: must run from inside the repository" >&2
    exit 1
}

check_mode=0
if [[ "${1:-}" == "--check" ]]; then
    check_mode=1
fi

version_file="$repo_root/internal/version/version.go"
version=$(sed -n 's/^const Version = "\(.*\)"$/\1/p' "$version_file")
if [[ -z "$version" ]]; then
    echo "bump-header: could not read Version from $version_file" >&2
    exit 1
fi

new_header="-- Compiled with sloptor v$version"
header_re='-- Compiled with (roblox-ts|@isentinel/roblox-ts|sloptor) v[0-9]+\.[0-9]+\.[0-9]+'

stale=0

replace_in() {
    local file="$1"
    local tmp
    tmp=$(mktemp) || { echo "bump-header: mktemp failed" >&2; exit 1; }

    perl -pe "s!${header_re}!${new_header}!g" "$file" > "$tmp" || {
        echo "bump-header: failed to rewrite $file" >&2
        rm -f "$tmp"
        return 1
    }

    if cmp -s "$file" "$tmp"; then
        rm -f "$tmp"
        return 0
    fi

    if [[ "$check_mode" == 1 ]]; then
        echo "bump-header: stale header in $file (expected $new_header)" >&2
        rm -f "$tmp"
        stale=1
        return 0
    fi

    mv "$tmp" "$file"
    echo "bump-header: updated $file"
}

# Go test files.
while IFS= read -r -d '' file; do
    replace_in "$file"
done < <(find "$repo_root/internal" "$repo_root/cmd/rotor" -type f -name '*_test.go' -print0 2>/dev/null)

# Golden Luau files under testdata and internal testdata (skip node_modules).
while IFS= read -r -d '' file; do
    replace_in "$file"
done < <(find "$repo_root/testdata" "$repo_root/internal/flamework/testdata" -type d \( -name node_modules -o -name .git \) -prune -o -type f -name '*.luau' -print0 2>/dev/null)

if [[ "$check_mode" == 1 && "$stale" == 1 ]]; then
    echo "bump-header: found stale headers; run without --check to fix" >&2
    exit 1
fi

echo "bump-header: headers are in sync with sloptor v$version"
