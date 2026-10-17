#!/usr/bin/env bash

set -euo pipefail

function error() {
	printf '%s\n' "$*" >&2
}

function get-first-executable-in-path() {
	local candidate

	for candidate in "$@"; do
		if command -v "${candidate}" > /dev/null 2>&1; then
			command -v "${candidate}"
			return 0
		fi
	done

	return 1
}

function get-first-existing-file() {
	local candidate

	for candidate in "$@"; do
		if [[ -n "${candidate}" && -x "${candidate}" ]]; then
			printf '%s\n' "${candidate}"
			return 0
		fi
	done

	return 1
}

function resolve-macos-chrome() {
	get-first-existing-file \
		"$HOME/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta" \
		"/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta" \
		"$HOME/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary" \
		"/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary" \
		"$HOME/Applications/Google Chrome Dev.app/Contents/MacOS/Google Chrome Dev" \
		"/Applications/Google Chrome Dev.app/Contents/MacOS/Google Chrome Dev" \
		"$HOME/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
		"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
		"$HOME/Applications/Chromium.app/Contents/MacOS/Chromium" \
		"/Applications/Chromium.app/Contents/MacOS/Chromium"
}

function resolve-linux-chrome() {
	get-first-executable-in-path \
		google-chrome-beta \
		google-chrome-stable \
		google-chrome \
		google-chrome-unstable \
		chromium \
		chromium-browser \
		chrome
}

function resolve-windows-chrome() {
	local candidate
	local commandOutput

	if ! command -v cmd.exe > /dev/null 2>&1; then
		return 1
	fi

	for candidate in \
		'%ProgramFiles%\Google\Chrome Beta\Application\chrome.exe' \
		'%ProgramFiles%\Google\Chrome\Application\chrome.exe' \
		'%ProgramFiles%\Google\Chrome Dev\Application\chrome.exe' \
		'%ProgramFiles%\Google\Chrome SxS\Application\chrome.exe' \
		'%ProgramFiles(x86)%\Google\Chrome Beta\Application\chrome.exe' \
		'%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe' \
		'%ProgramFiles(x86)%\Google\Chrome Dev\Application\chrome.exe' \
		'%ProgramFiles(x86)%\Google\Chrome SxS\Application\chrome.exe' \
		'%LocalAppData%\Google\Chrome Beta\Application\chrome.exe' \
		'%LocalAppData%\Google\Chrome\Application\chrome.exe' \
		'%LocalAppData%\Google\Chrome Dev\Application\chrome.exe' \
		'%LocalAppData%\Google\Chrome SxS\Application\chrome.exe'; do
		if commandOutput="$(cmd.exe /c "if exist \"${candidate}\" (echo ${candidate}) else exit /b 1" 2> /dev/null)"; then
			commandOutput="${commandOutput%$'\r'}"
			printf '%s\n' "${commandOutput}"
			return 0
		fi
	done

	return 1
}

function resolve-chrome-path() {
	local override
	local path
	local platform

	for override in "${CHROME_PATH:-}" "${GOOGLE_CHROME_PATH:-}" "${CHROME_BIN:-}"; do
		if [[ -n "${override:-}" && -x "${override}" ]]; then
			printf '%s\n' "${override}"
			return 0
		fi
	done

	platform="$(uname -s 2> /dev/null || printf '%s' "${OS:-}")"

	case "${platform}" in
		Darwin)
			if path="$(resolve-macos-chrome)"; then
				printf '%s\n' "${path}"
				return 0
			fi
			;;
		Linux)
			if path="$(resolve-linux-chrome)"; then
				printf '%s\n' "${path}"
				return 0
			fi
			;;
		MINGW* | MSYS* | CYGWIN* | Windows_NT)
			if path="$(resolve-windows-chrome)"; then
				printf '%s\n' "${path}"
				return 0
			fi
			;;
		*)
			error "Unsupported platform: ${platform}"
			return 1
			;;
	esac

	if path="$(get-first-executable-in-path \
		google-chrome-beta \
		google-chrome-stable \
		google-chrome \
		google-chrome-unstable \
		chromium \
		chromium-browser \
		chrome)"; then
		printf '%s\n' "${path}"
		return 0
	fi

	return 1
}

chromePath="$(resolve-chrome-path)" || {
	error "Unable to locate a Chrome executable."
	error "Set CHROME_PATH to override, or install Google Chrome / Chromium."
	exit 1
}

case "${1:---serve}" in
	--print-path)
		printf '%s\n' "${chromePath}"
		;;
	--serve | --launch)
		shift || true
		exec nlx chrome-devtools-mcp@latest --no-usage-statistics --executablePath "${chromePath}" "$@"
		;;
	*)
		error "Usage: $0 [--print-path|--serve]"
		exit 1
		;;
esac
