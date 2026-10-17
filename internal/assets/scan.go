package assets

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

// ScanResult is what Scan found under the configured globs.
type ScanResult struct {
	Files   []File   // matched, classified asset files, sorted by Path
	Skipped []string // matched files with unknown extensions, sorted
}

// Scan expands the configured glob patterns relative to projectDir, classifies
// each match by extension, and content-hashes the asset files. Files matched
// by multiple patterns are returned once. Patterns that match nothing are not
// an error. `.git` and `node_modules` directories are never descended into
// (unless a pattern's static prefix points inside one).
func Scan(projectDir string, patterns []string) (*ScanResult, error) {
	absProject, err := filepath.Abs(projectDir)
	if err != nil {
		return nil, fmt.Errorf("assets: resolving project dir: %w", err)
	}

	candidates := map[string]string{} // project-relative slash path -> abs path
	for _, raw := range patterns {
		pat := path.Clean(strings.ReplaceAll(strings.TrimSpace(raw), "\\", "/"))
		if pat == "" || pat == "." {
			continue
		}
		prefix := staticPrefix(pat)
		root := filepath.Join(absProject, filepath.FromSlash(prefix))
		info, err := os.Stat(root)
		if err != nil {
			continue // pattern matches nothing
		}
		if !info.IsDir() {
			// The whole pattern is a literal file path.
			if prefix == strings.Join(splitClean(pat), "/") {
				rel, err := relSlash(absProject, root)
				if err != nil {
					return nil, err
				}
				candidates[rel] = root
			}
			continue
		}
		walkErr := filepath.WalkDir(root, func(p string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				if name := d.Name(); p != root && (name == ".git" || name == "node_modules") {
					return fs.SkipDir
				}
				return nil
			}
			rel, err := relSlash(absProject, p)
			if err != nil {
				return err
			}
			if Match(pat, rel) {
				candidates[rel] = p
			}
			return nil
		})
		if walkErr != nil {
			return nil, fmt.Errorf("assets: scanning %q: %w", raw, walkErr)
		}
	}

	res := &ScanResult{}
	for rel, abs := range candidates {
		typ, ok := classifyExt(path.Ext(rel))
		if !ok {
			res.Skipped = append(res.Skipped, rel)
			continue
		}
		hash, err := hashFile(abs)
		if err != nil {
			return nil, fmt.Errorf("assets: hashing %s: %w", rel, err)
		}
		res.Files = append(res.Files, File{Path: rel, Abs: abs, Type: typ, Hash: hash})
	}
	sort.Slice(res.Files, func(i, j int) bool { return res.Files[i].Path < res.Files[j].Path })
	sort.Strings(res.Skipped)
	return res, nil
}

// relSlash returns p relative to root as a forward-slash path.
func relSlash(root, p string) (string, error) {
	rel, err := filepath.Rel(root, p)
	if err != nil {
		return "", err
	}
	return filepath.ToSlash(rel), nil
}

// HashFile returns "sha256:<hex>" of the file at p — the same content hash
// Scan records and the lockfile is keyed by. Shared by the $asset resolver
// (internal/assetresolve) so a build-time hash matches a sync-time hash
// exactly (a single source of truth for the cache key).
func HashFile(p string) (string, error) {
	f, err := os.Open(p)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }() // read-only; Close cannot lose data
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return "sha256:" + hex.EncodeToString(h.Sum(nil)), nil
}

// hashFile is the internal alias retained for Scan's call sites.
func hashFile(p string) (string, error) { return HashFile(p) }
