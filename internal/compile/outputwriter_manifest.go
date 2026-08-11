package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"path/filepath"
	"strings"
)

func (writer *outputWriter) validatedOutputHashes(outputs map[string]string) map[string]string {
	validated := make(map[string]string, len(outputs))
	for path, hash := range outputs {
		key, ok := normalizeOutputPresenceKey(path, writer.caseSensitive)
		if !ok {
			continue
		}
		info, err := writer.lstat(filepath.Join(writer.projectDir, filepath.FromSlash(key)))
		if err != nil || !info.Mode().IsRegular() {
			continue
		}
		var contents []byte
		if writer.root != nil {
			contents, err = writer.root.ReadFile(filepath.FromSlash(key))
		} else {
			contents, err = writer.operations.readFile(filepath.Join(writer.projectDir, filepath.FromSlash(key)))
		}
		if err != nil {
			continue
		}
		sum := sha256.Sum256(contents)
		if hex.EncodeToString(sum[:]) == hash {
			validated[path] = hash
		}
	}
	return validated
}

// outputPresenceIndex is a confined snapshot of which manifest output paths
// exist as regular files. It groups the requested paths by parent directory,
// opening and reading each parent exactly once instead of lstatting every
// output individually (the warm incremental path probes thousands of outputs
// in missing-output recovery and pruning).
type outputPresenceIndex struct {
	caseSensitive bool
	regular       map[string]struct{}
}

// normalizeOutputPresenceKey cleans and slash-normalizes a manifest output
// key, rejects non-local results, and lowercases the result only when the
// writer is case-insensitive.
func normalizeOutputPresenceKey(outputPath string, caseSensitive bool) (string, bool) {
	key := filepath.ToSlash(filepath.Clean(filepath.FromSlash(outputPath)))
	if !filepath.IsLocal(key) {
		return "", false
	}
	if !caseSensitive {
		key = strings.ToLower(key)
	}
	return key, true
}

// newOutputPresenceIndex indexes which of the given manifest output paths
// exist as regular files right now. Missing or unreadable parents, rejected
// non-local keys, directories, and final-segment symlinks are absent from the
// index; os.Root-accepted internal symlinks behave exactly as they do for
// outputWriter.lstat. When the writer has no os.Root (injected operations),
// every key is probed through the injected lstat to preserve test behavior.
func (writer *outputWriter) newOutputPresenceIndex(outputs map[string]string) *outputPresenceIndex {
	index := &outputPresenceIndex{
		caseSensitive: writer.caseSensitive,
		regular:       make(map[string]struct{}, len(outputs)),
	}
	if writer.root == nil {
		for path := range outputs {
			key, ok := normalizeOutputPresenceKey(path, writer.caseSensitive)
			if !ok {
				continue
			}
			info, err := writer.lstat(filepath.Join(writer.projectDir, filepath.FromSlash(key)))
			if err == nil && info.Mode().IsRegular() {
				index.regular[key] = struct{}{}
			}
		}
		return index
	}
	keysByParent := make(map[string][]string)
	for path := range outputs {
		key, ok := normalizeOutputPresenceKey(path, writer.caseSensitive)
		if !ok {
			continue
		}
		parent := filepath.Dir(key)
		keysByParent[parent] = append(keysByParent[parent], key)
	}
	for parent, keys := range keysByParent {
		dir, err := writer.root.Open(parent)
		if err != nil {
			continue
		}
		entries, readErr := dir.ReadDir(-1)
		_ = dir.Close()
		if readErr != nil {
			continue
		}
		regular := make(map[string]struct{}, len(entries))
		for _, entry := range entries {
			if !entry.Type().IsRegular() {
				continue
			}
			name := entry.Name()
			if !writer.caseSensitive {
				name = strings.ToLower(name)
			}
			regular[name] = struct{}{}
		}
		for _, key := range keys {
			base := filepath.Base(key)
			if !writer.caseSensitive {
				base = strings.ToLower(base)
			}
			if _, ok := regular[base]; ok {
				index.regular[key] = struct{}{}
			}
		}
	}
	return index
}

func (index *outputPresenceIndex) hasRegular(outputPath string) bool {
	key, ok := normalizeOutputPresenceKey(outputPath, index.caseSensitive)
	if !ok {
		return false
	}
	_, ok = index.regular[key]
	return ok
}
