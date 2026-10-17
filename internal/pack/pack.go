package pack

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// Format is the artifact rotor pack produces.
type Format string

const (
	FormatLuau  Format = "luau"  // self-reconstructing single Luau script
	FormatRbxmx Format = "rbxmx" // Roblox XML model (from rojo build)
	FormatRbxm  Format = "rbxm"  // Roblox binary model (from rojo build)
)

// Options configures Pack.
type Options struct {
	Project  string // a *.project.json file, or a directory containing one
	Format   Format
	Entry    string // luau only: dotted instance path; bundle returns require(entry)
	RojoTree bool   // luau only: force the rojo-built tree instead of the native one
}

// Pack builds the artifact bytes. For --as luau it builds the instance tree natively
// (no rojo) when the project is within the supported subset, falling back to
// `rojo build` otherwise; --as rbxmx/rbxm always use `rojo build` (faithful models).
func Pack(opts Options) ([]byte, error) {
	project, err := resolveProject(opts.Project)
	if err != nil {
		return nil, err
	}
	if opts.Format == FormatLuau {
		return packLuau(project, opts)
	}
	return buildModelViaRojo(project, string(opts.Format))
}

func packLuau(project string, opts Options) ([]byte, error) {
	if !opts.RojoTree {
		root, supported, err := BuildNative(project)
		if err != nil {
			return nil, err
		}
		if supported {
			return emitLuau([]*Instance{root}, opts.Entry)
		}
		// project uses features the native builder can't reproduce 1:1; fall back.
	}
	roots, err := buildTreeViaRojo(project)
	if err != nil {
		return nil, err
	}
	return emitLuau(roots, opts.Entry)
}

func emitLuau(roots []*Instance, entry string) ([]byte, error) {
	src, err := EmitLuau(roots, entry)
	if err != nil {
		return nil, err
	}
	return []byte(src), nil
}

// buildTreeViaRojo builds a .rbxmx with `rojo build` and parses it into the instance
// tree (the authoritative tree, with all of Rojo's middleware applied).
func buildTreeViaRojo(project string) ([]*Instance, error) {
	data, err := rojoBuildBytes(project, "rbxmx")
	if err != nil {
		return nil, err
	}
	roots, err := ParseRbxmx(data)
	if err != nil {
		return nil, fmt.Errorf("parsing rojo model: %w", err)
	}
	return roots, nil
}

func buildModelViaRojo(project, ext string) ([]byte, error) {
	return rojoBuildBytes(project, ext)
}

// rojoBuildBytes runs `rojo build` to a temp file with the given extension and
// returns the bytes.
func rojoBuildBytes(project, ext string) ([]byte, error) {
	rojoBin, err := exec.LookPath("rojo")
	if err != nil {
		return nil, fmt.Errorf("rojo is required for this project but is not on PATH " +
			"(the native builder handles plain script trees; .json/.model.json/.meta.json, " +
			"services, and nested projects need `rojo build`)")
	}
	tmp, err := os.CreateTemp("", "rotor-pack-*."+ext)
	if err != nil {
		return nil, err
	}
	tmpPath := tmp.Name()
	_ = tmp.Close()
	defer func() { _ = os.Remove(tmpPath) }() // best-effort temp cleanup

	cmd := exec.Command(rojoBin, "build", project, "-o", tmpPath)
	if out, err := cmd.CombinedOutput(); err != nil {
		return nil, fmt.Errorf("rojo build failed: %v\n%s", err, out)
	}
	return os.ReadFile(tmpPath)
}

func resolveProject(p string) (string, error) {
	if p == "" {
		p = "."
	}
	info, err := os.Stat(p)
	if err != nil {
		return "", fmt.Errorf("project path %q: %w", p, err)
	}
	if !info.IsDir() {
		return p, nil
	}
	if def := filepath.Join(p, "default.project.json"); isFile(def) {
		return def, nil
	}
	matches, _ := filepath.Glob(filepath.Join(p, "*.project.json"))
	if len(matches) > 0 {
		return matches[0], nil
	}
	return "", fmt.Errorf("no *.project.json found in %s", p)
}

func isFile(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}
