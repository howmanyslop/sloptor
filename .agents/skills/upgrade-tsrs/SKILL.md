---
name: upgrade-tsrs
description: tsrs upgrade. Re-vendor `third_party/tsrs` at a new upstream tag. Use when asked to move tsrs to a version, however the version is written (`tsrs@7.1.0-beta.tsrs3`, `v0.25.0`).
---

In this repo, "tsrs" means the vendored mirror in `third_party/tsrs`, which `crates/rotor_tsrs` wraps. An upgrade re-vendors that mirror at a new upstream tag and rebases the patches in `tools/tsrs-mirror/patches/`. The npm `tsrs` entry in `pnpm-workspace.yaml` is a separate pin for the type-check CLI. Leave it alone unless the user asks for it.

Run every cargo command through mise, as `mise x -- cargo …` or as a `mise run` task. mise sets the target directory and the build cache. Before the first build, run `mise x -- cargo --version`. If the output is not the toolchain that `mise.toml` pins, put the `bin` directory of that rustup toolchain first on `PATH`.

## Steps

1. **Resolve the pin.** Run `git ls-remote --tags https://github.com/oxc-project/tsrs.git <tag>` to get the commit. For an annotated tag, use the `^{}` line. Read the release notes of every release after the current `TSRS_TAG`, up to the new tag, with `gh release view <tag> -R oxc-project/tsrs`. Done when you know the tag with its commit and the Rust version that upstream requires, and you have listed every noted change to the compiler or to its memory management.

2. **Probe the patches.** Clone the new tag into a scratch directory. Fetch the old tag beside it for diffs. Apply the patches in name order with `git apply --verbose`. Done when every patch is marked as applied or has its rejected hunks listed.

3. **Rebase the rejected patches.** Keep the change that each patch makes: the same items made public, the same fields and functions added. Drop a hunk only when upstream now does what the hunk did. Keep the number, the name, and the header format of each file. Leave whitespace and line endings as they are. Done when all patches apply in order with no rejects, and the patches you regenerated apply with no offsets.

4. **Check every patch for semantic conflicts.** A patch that applies can still be wrong. For every patch, diff upstream between the two tags in each file that the patch touches. Read the changes near each hunk. Look for these cases:

   - A struct that the patch extends has a new field or a new construction site. Code that the patch adds must fill the new field the way upstream fills it.
   - A function that the patch calls has a new signature or a new meaning.
   - Ownership of memory moved. sloptor's long-lived sessions hold loader regions and checker regions through `ProgramScope`, and they share parsed files through `SolutionCompileCache`. Check any upstream change to what creates, holds, or frees a region or an arena against `crates/rotor_tsrs/src/project.rs`. The files to read are `program.rs`, `programregions.rs`, `checkerpool.rs`, and `tsrs_core/src/arena.rs`.

   Fix a conflict in a patch, and add a test that fails without the fix. Done when every patch has a one-line verdict: its meaning is the same, or it changed and the verdict says how. Take any change you cannot settle to the user.

5. **Re-vendor.** Set `TSRS_TAG` and `TSRS_COMMIT` in `tools/tsrs-mirror/mirror.ts`. Run `mise run tsrs:mirror -- --source <clone>` with the clone clean and at the pinned commit. If the tool fails on a new upstream layout, fix `mirror.ts` and add a test for that layout to `mirror.integration.test.ts`. Every change to `third_party/tsrs` comes from a patch and a new mirror run. Done when `third_party/tsrs/MIRROR.md` shows the new tag with its commit and lists every patch, and a second `mise run tsrs:mirror` leaves `git status` unchanged.

6. **Follow the toolchain.** If `third_party/tsrs/Cargo.toml` requires a newer Rust, change three places: `rust` in `mise.toml`, `rust-version` in the root `Cargo.toml`, and the Rust version inside `TAK_RUNNER` in `.github/workflows/perf.yaml` and `perf-pr.yaml`. Run `mise install`. Then run `cargo fmt` and `cargo clippy --fix`, and fix by hand what clippy leaves. Done when `mise run rust:lint` passes.

7. **Verify.** Run these in order:

   ```sh
   mise x -- cargo check --locked --workspace --all-targets
   mise run rust:lint
   mise run rust:test
   mise run tools:test
   ```

   Then run forkparity as [Limits of a local machine](#limits-of-a-local-machine) describes. If the test in `crates/rotor_tsrs/tests/oracle.rs` finds thousands of mismatches, the fixture dependencies are missing. `CONTRIBUTING.md` lists the directories to install them in. A golden diff or an oracle diff is a finding. Leave the golden as it is. Report the diff and the upstream change that causes it. Done when every command passes, or every failure has a cause you can state.

8. **Remove stale references.** Run `rg -n -F '<old version>'` outside `third_party/` and `CHANGELOG.md`. In `crates/rotor_tsrs/API.md`, remove the `(000N patch)` marker from each item whose hunk you dropped in step 3. Done when no remaining hit refers to the pin of the mirror.

9. **Commit each stage on its own.** Put the re-vendor and the toolchain in one commit. Put the format and lint changes in another. Put each fix in a commit with its patch and its test. In the PR body, list every check you did not run.

## Limits of a local machine

Leave the `release` profile to CI. The fat-LTO link of `release` needs more memory than a laptop has, and `mise run rust:forkparity` builds that profile. Run the suite against a `fastrel` build instead, at low priority:

```sh
nice -n 19 mise x -- sh -c '
	cargo build --locked --profile fastrel -p rotor_cli -j 3 || exit 1
	ROTOR_BIN="$(cd "${CARGO_TARGET_DIR:-target}" && pwd)/fastrel/sloptor"
	export ROTOR_BIN
	cargo nextest run --locked -p rotor_cli --test forkparity --run-ignored all --success-output final --test-threads 3
'
```

A suite that prints `SKIPPED` did not run. List it in the report. Windows and Linux builds also belong to CI.
