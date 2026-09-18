// Command mirror vendors microsoft/typescript-go's internal packages into ./tsgo
// with import paths rewritten so they are importable from the rotor module.
//
// Usage: go run ./tools/mirror [-ref main] [-repo URL]
package main

import (
	"bytes"
	"errors"
	"flag"
	"fmt"
	"io/fs"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const (
	srcModule  = "github.com/microsoft/typescript-go/internal/"
	dstModule  = "rotor/tsgo/"
	outDir     = "tsgo"
	overlayDir = "tools/mirror/overlay"
	patchDir   = "tools/mirror/patches"
)

type candidateError struct {
	dir string
	err error
}

func (e *candidateError) Error() string {
	return fmt.Sprintf("%v\nmirror candidate preserved at %s", e.err, e.dir)
}

func (e *candidateError) Unwrap() error { return e.err }

type replacementError struct {
	err       error
	candidate string
	original  string
	backup    string
}

func (e *replacementError) Error() string {
	var paths []string
	if e.candidate != "" {
		paths = append(paths, "candidate: "+e.candidate)
	}
	if e.original != "" {
		paths = append(paths, "original mirror: "+e.original)
	}
	if e.backup != "" {
		paths = append(paths, "backup: "+e.backup)
	}
	if len(paths) == 0 {
		return fmt.Sprintf("replace mirror: %v", e.err)
	}
	return fmt.Sprintf("replace mirror: %v\nrecovery paths:\n%s", e.err, strings.Join(paths, "\n"))
}

func (e *replacementError) Unwrap() error { return e.err }

func main() {
	repo := flag.String("repo", "https://github.com/microsoft/typescript-go", "source repo")
	ref := flag.String("ref", "main", "git ref to vendor")
	flag.Parse()

	nFiles, sha, err := mirror(*repo, *ref, outDir)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("mirrored %d files at %s\nnow run: go mod tidy && go build ./tsgo/...\n", nFiles, sha)
}

func mirror(repo, ref, destination string) (int, string, error) {
	source, err := os.MkdirTemp("", "tsgo-mirror-source-*")
	if err != nil {
		return 0, "", err
	}
	defer os.RemoveAll(source)

	if err := run(source, "git", "init", "-q"); err != nil {
		return 0, "", err
	}
	if err := run(source, "git", "remote", "add", "origin", repo); err != nil {
		return 0, "", err
	}
	if err := run(source, "git", "fetch", "-q", "--depth", "1", "origin", ref); err != nil {
		return 0, "", err
	}
	if err := run(source, "git", "checkout", "-q", "FETCH_HEAD"); err != nil {
		return 0, "", err
	}
	sha, err := output(source, "git", "rev-parse", "HEAD")
	if err != nil {
		return 0, "", err
	}
	sha = strings.TrimSpace(sha)

	candidate, err := os.MkdirTemp(filepath.Dir(destination), ".tsgo-mirror-")
	if err != nil {
		return 0, "", err
	}
	keepCandidate := true
	defer func() {
		if !keepCandidate {
			_ = os.RemoveAll(candidate)
		}
	}()

	candidateOut := filepath.Join(candidate, outDir)
	nFiles, err := copyInternal(filepath.Join(source, "internal"), candidateOut)
	if err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := copyFile(filepath.Join(source, "LICENSE"), filepath.Join(candidateOut, "LICENSE")); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := copyFile(filepath.Join(source, "NOTICE.txt"), filepath.Join(candidateOut, "NOTICE")); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := applyOverlay(candidateOut); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := writeMirrorMetadata(candidateOut, repo, sha); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := run(candidate, "git", "init", "-q"); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := run(candidate, "git", "add", outDir); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := seedPatchBases(candidate, filepath.Dir(destination)); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := applyPatchesTo(candidate); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := os.RemoveAll(filepath.Join(candidate, ".git")); err != nil {
		return 0, "", preserveCandidate(candidate, err)
	}
	if err := replaceOutput(candidateOut, destination); err != nil {
		var replacement *replacementError
		if errors.As(err, &replacement) {
			return 0, "", replacement
		}
		return 0, "", preserveCandidate(candidate, err)
	}
	keepCandidate = false
	return nFiles, sha, nil
}

func preserveCandidate(dir string, err error) error {
	return &candidateError{dir: dir, err: err}
}

func copyInternal(srcRoot, dstRoot string) (int, error) {
	nFiles := 0
	err := filepath.WalkDir(srcRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(srcRoot, path)
		if err != nil {
			return err
		}
		if d.IsDir() {
			if d.Name() == "testdata" {
				return filepath.SkipDir
			}
			return os.MkdirAll(filepath.Join(dstRoot, rel), 0o755)
		}
		if strings.HasSuffix(d.Name(), "_test.go") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if strings.HasSuffix(d.Name(), ".go") {
			data = bytes.ReplaceAll(data, []byte(srcModule), []byte(dstModule))
		}
		nFiles++
		return os.WriteFile(filepath.Join(dstRoot, rel), data, 0o644)
	})
	return nFiles, err
}

func writeMirrorMetadata(destination, repo, sha string) error {
	mirrorMD := fmt.Sprintf(`# Mirror of microsoft/typescript-go internals

- Source: %s
- Commit: %s
- Vendored: %s
- Changes: files copied from internal/ with import paths rewritten
  ("%s" -> "%s"); *_test.go files and testdata/ directories omitted.
  Rotor-owned edits are re-applied from tools/mirror/patches after
  import rewriting. Overlay shims from tools/mirror/overlay
  (e.g. checker/rotor_exports.go) are applied automatically.
  Regenerate a pinned ref with:
  go run ./tools/mirror -ref <sha>
`, repo, sha, time.Now().UTC().Format(time.RFC3339), srcModule, dstModule)
	return os.WriteFile(filepath.Join(destination, "MIRROR.md"), []byte(mirrorMD), 0o644)
}

func patchNames() ([]string, error) {
	entries, err := os.ReadDir(patchDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".patch") {
			names = append(names, filepath.Join(patchDir, entry.Name()))
		}
	}
	sort.Strings(names)
	return names, nil
}

func patchTargets(patches []string) ([]string, error) {
	if len(patches) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{})
	var targets []string
	for _, reverse := range []bool{false, true} {
		args := []string{"apply", "--numstat", "-z"}
		if reverse {
			args = append(args, "--reverse")
		}
		stats, err := output("", "git", append(args, patches...)...)
		if err != nil {
			return nil, err
		}
		for record := range strings.SplitSeq(stats, "\x00") {
			if record == "" {
				continue
			}
			fields := strings.SplitN(record, "\t", 3)
			if len(fields) != 3 {
				return nil, fmt.Errorf("invalid Git patch file record %q", record)
			}
			path := fields[2]
			if _, exists := seen[path]; !exists {
				seen[path] = struct{}{}
				targets = append(targets, filepath.FromSlash(path))
			}
		}
	}
	sort.Strings(targets)
	return targets, nil
}

// seedPatchBases stores the rewritten preimages named in each patch in the
// candidate repository's object database. git apply --3way can then merge
// harmless upstream context drift instead of treating it as an unresolvable
// patch failure.
func seedPatchBases(candidate, currentMirror string) error {
	patches, err := patchNames()
	if err != nil {
		return err
	}
	targets, err := patchTargets(patches)
	if err != nil {
		return err
	}
	bases, err := os.MkdirTemp("", "tsgo-patch-bases-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(bases)

	for _, target := range targets {
		data, err := os.ReadFile(filepath.Join(currentMirror, target))
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return fmt.Errorf("read patch baseline %s: %w", target, err)
		}
		path := filepath.Join(bases, target)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			return err
		}
	}
	resolvedBases, err := filepath.EvalSymlinks(bases)
	if err != nil {
		return err
	}
	for i := len(patches) - 1; i >= 0; i-- {
		if err := gitApply("", "--reverse", "--unsafe-paths", "--directory", resolvedBases, patches[i]); err != nil {
			return fmt.Errorf("recover patch baseline %s: %w", patches[i], err)
		}
	}
	for _, target := range targets {
		path := filepath.Join(resolvedBases, target)
		if _, err := os.Stat(path); os.IsNotExist(err) {
			continue
		} else if err != nil {
			return fmt.Errorf("read patch baseline %s: %w", target, err)
		}
		if _, err := output(candidate, "git", "hash-object", "-w", path); err != nil {
			return fmt.Errorf("store patch baseline %s: %w", target, err)
		}
	}
	return nil
}

func applyPatchesTo(repository string) error {
	patches, err := patchNames()
	if err != nil {
		return err
	}
	for _, patch := range patches {
		absolutePatch, err := filepath.Abs(patch)
		if err != nil {
			return err
		}
		if err := gitApply(repository, "--3way", "--index", "--unsafe-paths", absolutePatch); err != nil {
			return fmt.Errorf("apply %s: %w", patch, err)
		}
	}
	return nil
}

func gitApply(directory string, args ...string) error {
	cmd := exec.Command("git", append([]string{"apply"}, args...)...)
	cmd.Dir = directory
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w\n%s", err, out)
	}
	return nil
}

func replaceOutput(candidate, destination string) error {
	backup, err := os.MkdirTemp(filepath.Dir(destination), ".tsgo-mirror-previous-")
	if err != nil {
		return &replacementError{
			err:       err,
			candidate: survivingPath(candidate),
			original:  survivingPath(destination),
		}
	}
	if err := os.Remove(backup); err != nil {
		return &replacementError{
			err:       err,
			candidate: survivingPath(candidate),
			original:  survivingPath(destination),
			backup:    survivingPath(backup),
		}
	}
	if err := os.Rename(destination, backup); err != nil {
		if !errors.Is(err, fs.ErrNotExist) {
			return &replacementError{
				err:       err,
				candidate: survivingPath(candidate),
				original:  survivingPath(destination),
			}
		}
		backup = ""
	}
	if err := os.Rename(candidate, destination); err != nil {
		if backup != "" {
			if rollbackErr := os.Rename(backup, destination); rollbackErr != nil {
				return &replacementError{
					err:       fmt.Errorf("move candidate: %w; restore original: %w", err, rollbackErr),
					candidate: survivingPath(candidate),
					original:  backup,
					backup:    backup,
				}
			}
			return &replacementError{err: err, candidate: survivingPath(candidate), original: destination}
		}
		return &replacementError{err: err, candidate: survivingPath(candidate)}
	}
	if backup != "" {
		if err := os.RemoveAll(backup); err != nil {
			log.Printf("warning: mirrored tree replaced, but the previous mirror remains at %s: %v", backup, err)
		}
	}
	return nil
}

func survivingPath(path string) string {
	if _, err := os.Lstat(path); err == nil {
		return path
	}
	return ""
}

func applyOverlay(dst string) error {
	return filepath.WalkDir(overlayDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".tmpl") {
			return nil
		}
		rel, err := filepath.Rel(overlayDir, path)
		if err != nil {
			return err
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, strings.TrimSuffix(rel, ".tmpl"))
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
}

func run(dir string, name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("%s %v: %w", name, args, err)
	}
	return nil
}

func output(dir string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%s %v: %w", name, args, err)
	}
	return string(out), nil
}

func copyFile(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		alt := strings.TrimSuffix(src, ".txt")
		if data, err = os.ReadFile(alt); err != nil {
			log.Printf("warning: could not copy %s: %v", src, err)
			return nil
		}
	}
	return os.WriteFile(dst, data, 0o644)
}
