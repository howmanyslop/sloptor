package conformance

import (
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"rotor/internal/compile"
)

func resolveRandomnessProjectPath() (string, error) {
	raw := strings.TrimSpace(os.Getenv("ROTOR_RANDOMNESS_PATH"))
	if raw == "" {
		return "", fmt.Errorf("ROTOR_RANDOMNESS_PATH not set; point it at the randomness project root or its tsconfig.json")
	}
	info, err := os.Stat(raw)
	if err != nil {
		return "", fmt.Errorf("ROTOR_RANDOMNESS_PATH %q is not readable: %w", raw, err)
	}
	if !info.IsDir() {
		if filepath.Base(raw) != "tsconfig.json" {
			return "", fmt.Errorf("ROTOR_RANDOMNESS_PATH %q must be a project directory or tsconfig.json", raw)
		}
		raw = filepath.Dir(raw)
	}
	if _, err := os.Stat(filepath.Join(raw, "tsconfig.json")); err != nil {
		return "", fmt.Errorf("ROTOR_RANDOMNESS_PATH %q does not contain tsconfig.json: %w", raw, err)
	}
	return raw, nil
}

func TestResolveRandomnessProjectPath(t *testing.T) {
	t.Run("accepts project root", func(t *testing.T) {
		dir := t.TempDir()
		if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
		t.Setenv("ROTOR_RANDOMNESS_PATH", dir)
		got, err := resolveRandomnessProjectPath()
		if err != nil {
			t.Fatal(err)
		}
		if got != dir {
			t.Fatalf("path = %q, want %q", got, dir)
		}
	})

	t.Run("accepts tsconfig path", func(t *testing.T) {
		dir := t.TempDir()
		tsconfig := filepath.Join(dir, "tsconfig.json")
		if err := os.WriteFile(tsconfig, []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
		t.Setenv("ROTOR_RANDOMNESS_PATH", tsconfig)
		got, err := resolveRandomnessProjectPath()
		if err != nil {
			t.Fatal(err)
		}
		if got != dir {
			t.Fatalf("path = %q, want %q", got, dir)
		}
	})
}

func TestNormalizeCompiledHeader(t *testing.T) {
	input := "-- Compiled with rotor v0.1.0-dev\nprint(\"ok\")\n"
	got := normalizeCompiledHeader(input)
	want := "-- Compiled with <normalized>\nprint(\"ok\")\n"
	if got != want {
		t.Fatalf("normalizeCompiledHeader = %q, want %q", got, want)
	}
}

func TestCompareOutputTrees(t *testing.T) {
	left := t.TempDir()
	right := t.TempDir()

	for _, root := range []string{left, right} {
		if err := os.MkdirAll(filepath.Join(root, "out"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.MkdirAll(filepath.Join(root, "include"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(root, "out", "main.luau"), []byte("-- Compiled with rotor v0.1.0-dev\nprint(\"ok\")\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(root, "include", "RuntimeLib.lua"), []byte("return {}\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	if err := os.WriteFile(filepath.Join(right, "out", "main.luau"), []byte("-- Compiled with sloptor v2.3.3\nprint(\"ok\")\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	if diffs, err := compareOutputTrees(
		[]string{filepath.Join(left, "out"), filepath.Join(left, "include")},
		[]string{filepath.Join(right, "out"), filepath.Join(right, "include")},
	); err != nil {
		t.Fatal(err)
	} else if len(diffs) != 0 {
		t.Fatalf("compareOutputTrees diffs = %v, want none", diffs)
	}
}

func TestStageAcceptanceProjectCopySkipsGeneratedDirs(t *testing.T) {
	src := t.TempDir()
	for _, dir := range []string{
		filepath.Join(src, "src"),
		filepath.Join(src, "node_modules"),
		filepath.Join(src, "out"),
		filepath.Join(src, "include"),
	} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(src, "tsconfig.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "src", "main.ts"), []byte("export {};\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "node_modules", "sentinel.txt"), []byte("node_modules\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "out", "stale.luau"), []byte("stale\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "include", "RuntimeLib.lua"), []byte("stale\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	dst := filepath.Join(t.TempDir(), "staged")
	if err := stageAcceptanceProjectCopy(src, dst); err != nil {
		t.Fatal(err)
	}

	if _, err := os.Stat(filepath.Join(dst, "src", "main.ts")); err != nil {
		t.Fatalf("staged source missing: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dst, "out")); !os.IsNotExist(err) {
		t.Fatalf("staged out/ err = %v, want not-exist", err)
	}
	if _, err := os.Stat(filepath.Join(dst, "include")); !os.IsNotExist(err) {
		t.Fatalf("staged include/ err = %v, want not-exist", err)
	}
	if _, err := os.Stat(filepath.Join(dst, "node_modules", "sentinel.txt")); err != nil {
		t.Fatalf("staged node_modules missing: %v", err)
	}
}

func TestRandomnessAcceptance(t *testing.T) {
	path, err := resolveRandomnessProjectPath()
	if err != nil {
		t.Skip(err.Error())
	}
	if _, err := os.Stat(filepath.Join(path, "tsconfig.json")); err != nil {
		t.Fatalf("randomness tsconfig missing: %v", err)
	}

	rotorCopy := filepath.Join(t.TempDir(), "rotor")
	rbxtscCopy := filepath.Join(t.TempDir(), "rbxtsc")
	if err := stageAcceptanceProjectCopy(path, rotorCopy); err != nil {
		t.Fatalf("copy rotor project: %v", err)
	}
	if err := stageAcceptanceProjectCopy(path, rbxtscCopy); err != nil {
		t.Fatalf("copy rbxtsc project: %v", err)
	}

	result, diags, err := compile.BuildProjectWithOptions(rotorCopy, compile.ProjectOptions{
		EmitIncludeFiles:       true,
		AllowCommentDirectives: true,
	})
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}
	if result == nil || len(result.Outputs) == 0 {
		t.Fatal("no emitted files")
	}

	if err := runRbxtscBuild(rbxtscCopy); err != nil {
		t.Fatalf("rbxtsc build: %v", err)
	}

	outputRel, err := filepath.Rel(rotorCopy, result.OutputDir)
	if err != nil {
		t.Fatalf("rel output dir: %v", err)
	}
	diffs, err := compareOutputTrees(
		[]string{result.OutputDir, filepath.Join(rotorCopy, "include")},
		[]string{filepath.Join(rbxtscCopy, outputRel), filepath.Join(rbxtscCopy, "include")},
	)
	if err != nil {
		t.Fatal(err)
	}
	if len(diffs) > 0 {
		t.Fatalf("randomness output differs from rbxtsc:\n%s", strings.Join(diffs, "\n"))
	}
}

func runRbxtscBuild(projectDir string) error {
	cmdName := "npx"
	args := []string{"rbxtsc"}
	localBin := filepath.Join(projectDir, "node_modules", ".bin", "rbxtsc.cmd")
	if _, err := os.Stat(localBin); err == nil {
		cmdName = "cmd"
		args = []string{"/c", localBin}
	}
	cmd := exec.Command(cmdName, args...)
	cmd.Dir = projectDir
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func compareOutputTrees(leftRoots, rightRoots []string) ([]string, error) {
	left, err := collectOutputFiles(leftRoots)
	if err != nil {
		return nil, err
	}
	right, err := collectOutputFiles(rightRoots)
	if err != nil {
		return nil, err
	}

	keys := make([]string, 0, len(left)+len(right))
	seen := make(map[string]struct{}, len(left)+len(right))
	for rel := range left {
		keys = append(keys, rel)
		seen[rel] = struct{}{}
	}
	for rel := range right {
		if _, ok := seen[rel]; ok {
			continue
		}
		keys = append(keys, rel)
	}
	slices.Sort(keys)

	var diffs []string
	for _, rel := range keys {
		leftPath, leftOK := left[rel]
		rightPath, rightOK := right[rel]
		if !leftOK || !rightOK {
			diffs = append(diffs, fmt.Sprintf("%s presence mismatch (left=%v right=%v)", rel, leftOK, rightOK))
			continue
		}
		leftBytes, err := os.ReadFile(leftPath)
		if err != nil {
			return nil, err
		}
		rightBytes, err := os.ReadFile(rightPath)
		if err != nil {
			return nil, err
		}
		leftText := string(leftBytes)
		rightText := string(rightBytes)
		if strings.HasSuffix(rel, ".lua") || strings.HasSuffix(rel, ".luau") {
			leftText = normalizeCompiledHeader(leftText)
			rightText = normalizeCompiledHeader(rightText)
		}
		if leftText != rightText {
			diffs = append(diffs, fmt.Sprintf("%s content mismatch", rel))
		}
	}
	return diffs, nil
}

func collectOutputFiles(roots []string) (map[string]string, error) {
	files := map[string]string{}
	for _, root := range roots {
		info, err := os.Stat(root)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return nil, err
		}
		if !info.IsDir() {
			return nil, fmt.Errorf("%q is not a directory", root)
		}
		base := filepath.Base(root)
		err = filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				return nil
			}
			if strings.HasSuffix(path, ".tsbuildinfo") {
				return nil
			}
			rel, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			files[filepath.ToSlash(filepath.Join(base, rel))] = path
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	return files, nil
}

func stageAcceptanceProjectCopy(src, dst string) error {
	if err := copyTreeFiltered(src, dst, func(rel string, d fs.DirEntry) bool {
		rel = filepath.ToSlash(rel)
		switch rel {
		case "node_modules", "out", "include", ".git":
			return false
		}
		return !strings.HasPrefix(rel, "node_modules/") &&
			!strings.HasPrefix(rel, "out/") &&
			!strings.HasPrefix(rel, "include/") &&
			!strings.HasPrefix(rel, ".git/")
	}); err != nil {
		return err
	}

	srcNodeModules := filepath.Join(src, "node_modules")
	if _, err := os.Stat(srcNodeModules); err == nil {
		dstNodeModules := filepath.Join(dst, "node_modules")
		if err := os.MkdirAll(filepath.Dir(dstNodeModules), 0o755); err != nil {
			return err
		}
		if err := os.Symlink(srcNodeModules, dstNodeModules); err == nil {
			return nil
		}
		if err := createJunction(srcNodeModules, dstNodeModules); err == nil {
			return nil
		}
		return copyTree(srcNodeModules, dstNodeModules)
	}
	return nil
}

func normalizeCompiledHeader(text string) string {
	const prefix = "-- Compiled with "
	if !strings.HasPrefix(text, prefix) {
		return text
	}
	if newline := strings.IndexByte(text, '\n'); newline >= 0 {
		return "-- Compiled with <normalized>\n" + text[newline+1:]
	}
	return "-- Compiled with <normalized>"
}

func copyTreeFiltered(src, dst string, keep func(rel string, d fs.DirEntry) bool) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return os.MkdirAll(dst, 0o755)
		}
		if !keep(rel, d) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		return copyFile(path, target)
	})
}

func createJunction(target, path string) error {
	cmd := exec.Command("cmd", "/c", "mklink", "/J", path, target)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}
