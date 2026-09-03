package compile

import (
	"encoding/json"
	"sync"

	"rotor/internal/logservice"
)

// afterDeclarationsWarned holds the projects that already reported an
// unsupported `afterDeclarations` transformer, so the warning is one line per
// project rather than one per round trip (a watch session round-trips on every
// keystroke).
var afterDeclarationsWarned sync.Map

// afterDeclarationsScanned holds the projects whose tsconfig chain has already
// been read for the `"afterDeclarations": true` flag. It is separate from
// afterDeclarationsWarned on purpose: the common answer is zero, and keying the
// scan off the warning sentinel would re-parse the whole extends chain on every
// round trip of every project that has no such plugin.
var afterDeclarationsScanned sync.Map

// warnUnsupportedAfterDeclarations reports, once per project, that the project
// declares `afterDeclarations` transformers rotor will not run.
//
// rotor emits declarations natively from tsgo, which has no custom-transformer
// hook, so an `afterDeclarations` transformer has nowhere to run. No plugin in
// the ecosystem rotor targets ships one; the warning exists so a project that
// does gets told rather than silently losing the transform.
func warnUnsupportedAfterDeclarations(configPath string, count int) {
	if count <= 0 {
		return
	}
	if _, warned := afterDeclarationsWarned.LoadOrStore(normalizeSourceFilePath(configPath), struct{}{}); warned {
		return
	}
	logservice.Warn(configPath + ": afterDeclarations transformers are not supported; declarations are emitted natively")
}

// takeAfterDeclarationsScan reports whether the caller owns this project's
// one tsconfig-chain scan, claiming it in the same step. It answers false once
// the scan has run (whatever it found) and once the warning is already out.
func takeAfterDeclarationsScan(configPath string) bool {
	key := normalizeSourceFilePath(configPath)
	if _, warned := afterDeclarationsWarned.Load(key); warned {
		return false
	}
	_, scanned := afterDeclarationsScanned.LoadOrStore(key, struct{}{})
	return !scanned
}

// countConfiguredAfterDeclarations counts plugin entries whose tsconfig object
// carries `"afterDeclarations": true`. This is the cheap half of the check: it
// sees a plugin that fails to load (which the worker's post-flatten count
// cannot), and it costs one JSON scan of an already-parsed entry.
//
// The other half — a factory that RETURNS `{ afterDeclarations }` without the
// config flag — is only visible to the worker, which reports its own count.
func countConfiguredAfterDeclarations(plugins []json.RawMessage) int {
	count := 0
	for _, plugin := range plugins {
		var entry struct {
			AfterDeclarations bool `json:"afterDeclarations"`
		}
		if json.Unmarshal(plugin, &entry) == nil && entry.AfterDeclarations {
			count++
		}
	}
	return count
}
