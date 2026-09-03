package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// transformerPluginFingerprint identifies one transformer plugin closely
// enough that changing it invalidates the incremental manifest.
//
// The compiler options the salt already covers say nothing about a
// transformer: tsgo drops `compilerOptions.plugins` while parsing options, and
// the plugin's own code lives outside the project. Without this, upgrading a
// transformer would leave every previously compiled file looking unchanged,
// so an incremental build would emit the new transformer's output for the
// handful of edited files and keep the old transformer's output for the rest.
type transformerPluginFingerprint struct {
	// Transform is the specifier the tsconfig names.
	Transform string `json:"transform"`
	// Config is the plugin's whole tsconfig entry, so an option change (a
	// different `import`, a flipped `after`) is a change.
	Config json.RawMessage `json:"config,omitempty"`
	// Version is the resolved package's declared version.
	Version string `json:"version,omitempty"`
	// EntryHash is the sha256 of the entry file the worker would require.
	EntryHash string `json:"entryHash,omitempty"`
	// Unresolved records why the package could not be pinned down, so a
	// project whose plugin does not resolve stays consistent with itself
	// instead of silently sharing a fingerprint with a resolvable one.
	Unresolved string `json:"unresolved,omitempty"`
}

// transformerPluginFingerprints describes the effective transformer plugin
// list of configPath, in declaration order.
//
// The version alone would miss a locally edited or linked plugin, and the
// entry-file hash alone would miss an upgrade whose entry is a thin
// re-export. Together they catch both. The hash covers the ENTRY FILE only,
// not the whole package: hashing a transformer's entire dependency tree on
// every build would cost more than the rebuild it protects. A plugin upgrade
// therefore forces a full rebuild of every project that names it, which is the
// intended trade.
func transformerPluginFingerprints(projectDir, configPath string) ([]transformerPluginFingerprint, error) {
	if configPath == "" {
		return nil, nil
	}
	plugins, err := effectiveTransformerPlugins(configPath)
	if err != nil {
		return nil, err
	}
	if len(plugins) == 0 {
		return nil, nil
	}
	fingerprints := make([]transformerPluginFingerprint, 0, len(plugins))
	for _, plugin := range plugins {
		fingerprints = append(fingerprints, fingerprintTransformerPlugin(projectDir, plugin))
	}
	return fingerprints, nil
}

func fingerprintTransformerPlugin(projectDir string, plugin transformerPluginConfig) transformerPluginFingerprint {
	fingerprint := transformerPluginFingerprint{Transform: plugin.Transform, Config: plugin.marshalJSON()}
	entryPath, version, reason := resolveTransformerPluginEntry(projectDir, plugin.Transform)
	if reason != "" {
		fingerprint.Unresolved = reason
		return fingerprint
	}
	fingerprint.Version = version
	contents, err := os.ReadFile(entryPath)
	if err != nil {
		fingerprint.Unresolved = "entry unreadable"
		return fingerprint
	}
	sum := sha256.Sum256(contents)
	fingerprint.EntryHash = hex.EncodeToString(sum[:])
	return fingerprint
}

// resolveTransformerPluginEntry mirrors the worker's
// `require.resolve(transform, { paths: [projectDir] })`
// (tools/sidecar/lib/plugins.js) closely enough to name the entry file and the
// declared version. It answers the two shapes a transformer is published in —
// a path inside the project, or a package under a `node_modules` directory at
// or above the project — and reports why it gave up otherwise.
func resolveTransformerPluginEntry(projectDir, transform string) (entryPath string, version string, unresolved string) {
	projectDir = filepath.Clean(filepath.FromSlash(projectDir))
	if transform == "" {
		return "", "", "empty transform"
	}
	if strings.HasPrefix(transform, ".") || filepath.IsAbs(filepath.FromSlash(transform)) {
		candidate := filepath.FromSlash(transform)
		if !filepath.IsAbs(candidate) {
			candidate = filepath.Join(projectDir, candidate)
		}
		resolved, ok := resolveModuleFile(candidate)
		if !ok {
			return "", "", "entry not found"
		}
		return resolved, "", ""
	}

	packageName, subpath := splitPackageSpecifier(transform)
	packageDir, ok := findPackageDirectory(projectDir, packageName)
	if !ok {
		return "", "", "package not installed"
	}
	manifest, err := readPackageManifest(packageDir)
	if err != nil {
		return "", "", "package.json unreadable"
	}
	entry := subpath
	if entry == "" {
		// resolvePackageExports already walks `exports` conditions and falls
		// back to `main`, which is exactly the runtime entry a `require` of
		// the bare package name lands on.
		_, runtimePath, exportsErr := resolvePackageExports(packageDir, nil)
		if exportsErr != nil || runtimePath == "" {
			entry = manifest.Main
		} else {
			entry = runtimePath
		}
	}
	if entry == "" {
		entry = "index.js"
	}
	candidate := filepath.FromSlash(entry)
	if !filepath.IsAbs(candidate) {
		candidate = filepath.Join(packageDir, candidate)
	}
	resolved, ok := resolveModuleFile(candidate)
	if !ok {
		return "", "", "entry not found"
	}
	return resolved, manifest.Version, ""
}

// splitPackageSpecifier separates `@scope/name/sub/path` into the package name
// and the subpath the specifier asked for.
func splitPackageSpecifier(specifier string) (packageName, subpath string) {
	parts := strings.Split(specifier, "/")
	take := 1
	if strings.HasPrefix(specifier, "@") && len(parts) > 1 {
		take = 2
	}
	if len(parts) <= take {
		return specifier, ""
	}
	return strings.Join(parts[:take], "/"), strings.Join(parts[take:], "/")
}

func findPackageDirectory(startDir, packageName string) (string, bool) {
	directory := startDir
	for {
		candidate := filepath.Join(directory, "node_modules", filepath.FromSlash(packageName))
		if info, err := os.Stat(filepath.Join(candidate, "package.json")); err == nil && !info.IsDir() {
			return candidate, true
		}
		parent := filepath.Dir(directory)
		if parent == directory {
			return "", false
		}
		directory = parent
	}
}

// resolveModuleFile applies the extension and index lookups Node would, so a
// `main` of `out/src/index` or of a directory still names a file.
func resolveModuleFile(candidate string) (string, bool) {
	if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
		return candidate, true
	}
	for _, extension := range []string{".js", ".cjs", ".mjs", ".ts"} {
		withExtension := candidate + extension
		if info, err := os.Stat(withExtension); err == nil && !info.IsDir() {
			return withExtension, true
		}
	}
	for _, index := range []string{"index.js", "index.cjs", "index.mjs"} {
		inDirectory := filepath.Join(candidate, index)
		if info, err := os.Stat(inDirectory); err == nil && !info.IsDir() {
			return inDirectory, true
		}
	}
	return "", false
}
