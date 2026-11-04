#!/usr/bin/env bash
# Keeps emitted `-- Compiled with sloptor v...` header lines in golden files
# and Rust test expectations in sync with
# crates/rotor_support/src/version.rs.
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

version_file="$repo_root/crates/rotor_support/src/version.rs"
version=$(sed -n 's/^pub const VERSION: &str = "\(.*\)";$/\1/p' "$version_file")
if [[ -z "$version" ]]; then
    echo "bump-header: could not read Version from $version_file" >&2
    exit 1
fi

new_header="-- Compiled with sloptor v$version"
header_re='-- Compiled with (roblox-ts|@isentinel/roblox-ts|sloptor) v[0-9]+\.[0-9]+\.[0-9]+(-[-0-9A-Za-z.]+)?(\+[-0-9A-Za-z.]+)?'

stale=0

replace_in() {
    local file="$1"
    local tmp
    tmp=$(mktemp) || {
        echo "bump-header: mktemp failed" >&2
        exit 1
    }

    perl -pe "s!${header_re}!${new_header}!g" "$file" >"$tmp" || {
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

# Rust test files (the crates read their current version from version.rs, so
# only the literal headers of expected output need rewriting).
while IFS= read -r -d '' file; do
    replace_in "$file"
done < <(find "$repo_root/crates" -type f -name '*.rs' -print0 2>/dev/null)

# Golden Luau files under testdata (skip node_modules). The transformer, compile and spike unit
# fixtures keep the headers their tests were written against, so they are left alone.
while IFS= read -r -d '' file; do
    replace_in "$file"
done < <(find "$repo_root/testdata" \( -type d \( -name node_modules -o -name .git \) -o -path "$repo_root/testdata/compile-unit" -o -path "$repo_root/testdata/spike-unit" -o -path "$repo_root/testdata/transformer-unit" \) -prune -o -type f -name '*.luau' -print0 2>/dev/null)

if [[ "$check_mode" == 1 && "$stale" == 1 ]]; then
    echo "bump-header: found stale headers; run without --check to fix" >&2
    exit 1
fi

echo "bump-header: headers are in sync with sloptor v$version"
