#!/usr/bin/env bash

# Run `tak` inside a Linux container so it can count instructions with valgrind
# (valgrind does not run on macOS).
#
# Usage:
#   scripts/tak-linux/tak-linux.sh doctor
#   scripts/tak-linux/tak-linux.sh run --record
#   scripts/tak-linux/tak-linux.sh --rebuild-image
#
# Everything after the script name is passed to `tak` unchanged. The repo is
# bind-mounted at /work, so `--record` writes refs/notes/tak into your real
# .git. Run `tak push` from the host: the container has no git credentials.
#
# Anything tak benchmarks must be a Linux binary. Do not build into the host's
# target/ from here; point CARGO_TARGET_DIR somewhere else.
set -euo pipefail

readonly image="tak-valgrind"
# Apple's vmnet resolver at the gateway refuses queries, so name one explicitly.
readonly dns="${TAK_LINUX_DNS:-1.1.1.1}"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "${script_dir}" rev-parse --show-toplevel)"
readonly script_dir repo_root

command -v container > /dev/null 2>&1 || {
	echo "error: \`container\` not found. Run via \`mise exec -- $0\` or activate mise." >&2
	exit 1
}

build_image() {
	container build --dns "${dns}" -t "${image}" \
		-f "${script_dir}/Dockerfile" "${script_dir}"
}

if [ "${1:-}" = "--rebuild-image" ]; then
	build_image
	exit 0
fi

container system status > /dev/null 2>&1 || container system start

container image inspect "${image}" > /dev/null 2>&1 || build_image

tty_flags=()
if [ -t 0 ] && [ -t 1 ]; then
	tty_flags=(-i -t)
fi

exec container run --rm --dns "${dns}" ${tty_flags[@]+"${tty_flags[@]}"} \
	-v "${repo_root}:/work" -w /work \
	"${image}" tak "$@"
