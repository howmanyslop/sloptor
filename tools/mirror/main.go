// Command mirror vendors microsoft/typescript-go's internal packages into ./tsgo
// with import paths rewritten so they are importable from the rotor module.
//
// Usage: go run ./tools/mirror [-ref main] [-repo URL]
package main

import (
	"bytes"
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

func main() {
	repo := flag.String("repo", "https://github.com/microsoft/typescript-go", "source repo")
	ref := flag.String("ref", "main", "git ref to vendor")
	flag.Parse()

	tmp, err := os.MkdirTemp("", "tsgo-mirror-*")
	if err != nil {
		log.Fatal(err)
	}
	defer os.RemoveAll(tmp)

	run(tmp, "git", "init", "-q")
	run(tmp, "git", "remote", "add", "origin", *repo)
	run(tmp, "git", "fetch", "-q", "--depth", "1", "origin", *ref)
	run(tmp, "git", "checkout", "-q", "FETCH_HEAD")
	sha := strings.TrimSpace(output(tmp, "git", "rev-parse", "HEAD"))

	if err := os.RemoveAll(outDir); err != nil {
		log.Fatal(err)
	}

	srcRoot := filepath.Join(tmp, "internal")
	nFiles := 0
	err = filepath.WalkDir(srcRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(srcRoot, path)
		if d.IsDir() {
			// skip test fixtures wholesale
			if d.Name() == "testdata" {
				return filepath.SkipDir
			}
			return os.MkdirAll(filepath.Join(outDir, rel), 0o755)
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
		return os.WriteFile(filepath.Join(outDir, rel), data, 0o644)
	})
	if err != nil {
		log.Fatal(err)
	}

	// Apache-2.0 obligations: license, notice, statement of changes.
	copyFile(filepath.Join(tmp, "LICENSE"), filepath.Join(outDir, "LICENSE"))
	copyFile(filepath.Join(tmp, "NOTICE.txt"), filepath.Join(outDir, "NOTICE"))

	// Rotor shims (e.g. checker/rotor_exports.go) live in tools/mirror/overlay
	// and are re-applied on every regeneration.
	if err := applyOverlay(outDir); err != nil {
		log.Fatal(err)
	}
	if err := applyPatchesTo(""); err != nil {
		log.Fatal(err)
	}

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
`, *repo, sha, time.Now().UTC().Format(time.RFC3339), srcModule, dstModule)
	if err := os.WriteFile(filepath.Join(outDir, "MIRROR.md"), []byte(mirrorMD), 0o644); err != nil {
		log.Fatal(err)
	}

	fmt.Printf("mirrored %d files at %s\nnow run: go mod tidy && go build ./tsgo/...\n", nFiles, sha)
}

// applyOverlay copies every *.tmpl file under overlayDir into dst at its
// relative path minus the ".tmpl" suffix. Overlay files are rotor-owned shims
// (e.g. checker/rotor_exports.go) that must survive regeneration; they are
// stored with the .tmpl suffix so `go build ./...` does not try to compile
// them in place.
func applyPatchesTo(directory string) error {
	entries, err := os.ReadDir(patchDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".patch") {
			continue
		}
		names = append(names, entry.Name())
	}
	sort.Strings(names)
	for _, name := range names {
		path := filepath.Join(patchDir, name)
		args := []string{"apply", "--unsafe-paths"}
		if directory != "" {
			resolved, err := filepath.EvalSymlinks(directory)
			if err != nil {
				return fmt.Errorf("resolve patch directory %q: %w", directory, err)
			}
			args = append(args, "--directory", resolved)
		}
		args = append(args, path)
		cmd := exec.Command("git", args...)
		out, err := cmd.CombinedOutput()
		if err != nil {
			return fmt.Errorf("apply %s: %w\n%s", name, err, out)
		}
	}
	return nil
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

func run(dir string, name string, args ...string) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		log.Fatalf("%s %v: %v", name, args, err)
	}
}

func output(dir string, name string, args ...string) string {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		log.Fatalf("%s %v: %v", name, args, err)
	}
	return string(out)
}

func copyFile(src, dst string) {
	data, err := os.ReadFile(src)
	if err != nil {
		// NOTICE may not exist under that exact name; try NOTICE without extension
		alt := strings.TrimSuffix(src, ".txt")
		if data2, err2 := os.ReadFile(alt); err2 == nil {
			data = data2
		} else {
			log.Printf("warning: could not copy %s: %v", src, err)
			return
		}
	}
	if err := os.WriteFile(dst, data, 0o644); err != nil {
		log.Fatal(err)
	}
}
