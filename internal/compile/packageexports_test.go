package compile

import (
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/rojo"
	"rotor/internal/transformer"
)

func TestResolvePackageExports(t *testing.T) {
	tests := []struct {
		name        string
		manifest    string
		wantTypes   string
		wantRuntime string
	}{
		{
			name:        "uses types and the first runtime condition",
			manifest:    `{"exports":{".":{"types":"./dist/index.d.ts","import":"./dist/import.luau","require":"./dist/require.luau","default":"./dist/default.luau"}}}`,
			wantTypes:   "dist/index.d.ts",
			wantRuntime: "dist/import.luau",
		},
		{
			name:        "uses an adjacent declaration when exports omits types",
			manifest:    `{"exports":"./dist/init.luau"}`,
			wantTypes:   "dist/init.d.ts",
			wantRuntime: "dist/init.luau",
		},
		{
			name:        "falls back to legacy types before typings and main",
			manifest:    `{"types":"./types/index.d.ts","typings":"./typings/index.d.ts","main":"./out/init.lua"}`,
			wantTypes:   "types/index.d.ts",
			wantRuntime: "out/init.lua",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pkgDir := writePackageManifest(t, t.TempDir(), "pkg", tt.manifest)

			typesPath, runtimePath, err := resolvePackageExports(pkgDir, nil)

			if err != nil {
				t.Fatalf("resolvePackageExports: %v", err)
			}
			if want := filepath.Join(pkgDir, filepath.FromSlash(tt.wantTypes)); typesPath != want {
				t.Errorf("types path = %q, want %q", typesPath, want)
			}
			if want := filepath.Join(pkgDir, filepath.FromSlash(tt.wantRuntime)); runtimePath != want {
				t.Errorf("runtime path = %q, want %q", runtimePath, want)
			}
		})
	}
}

func TestPackageExportsMapping(t *testing.T) {
	t.Run("maps subpaths and custom condition targets", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				".": {
					"types": "./dist/index.d.ts",
					"source": "./src/index.ts",
					"import": "./dist/init.luau"
				},
				"./utils": {
					"types": "./dist/utils.d.ts",
					"require": "./dist/utils.luau"
				},
				"./features/*": {
					"types": "./dist/features/*.d.ts",
					"import": "./dist/features/*.luau"
				}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, []string{"source"})

		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "dist", "index.d.ts"), filepath.Join(pkgDir, "dist", "init.luau"))
		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "src", "index.ts"), filepath.Join(pkgDir, "dist", "init.luau"))
		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "dist", "utils.d.ts"), filepath.Join(pkgDir, "dist", "utils.luau"))
		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "dist", "features", "a", "b.d.ts"), filepath.Join(pkgDir, "dist", "features", "a", "b.luau"))
		if _, exists := mapping.Exact[rojo.CanonicalFileName(filepath.Join(pkgDir, "dist", "features", "*.d.ts"), true)]; exists {
			t.Error("wildcard export subpath should not create an exact mapping")
		}
	})

	t.Run("remaps runtime through a package rojo path", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				".": {"types":"./dist/index.d.ts","import":"./dist/init.luau"}
			}
		}`)
		writePackageFile(t, pkgDir, "default.project.json", `{"name":"pkg","tree":{"$path":"out"}}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, nil)

		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "dist", "index.d.ts"), filepath.Join(pkgDir, "out", "init.luau"))
	})

	t.Run("adds a real path key for a symlinked package", func(t *testing.T) {
		dir := t.TempDir()
		realPkgDir := writePackageManifest(t, filepath.Join(dir, "store"), "pkg", `{
			"exports": {
				".": {"types":"./dist/index.d.ts","import":"./dist/init.luau"}
			}
		}`)
		scopePath := filepath.Join(dir, "node_modules", "@rbxts")
		virtualPkgDir := filepath.Join(scopePath, "pkg")
		if err := os.MkdirAll(scopePath, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(realPkgDir, virtualPkgDir); err != nil {
			t.Skipf("cannot create package symlink: %v", err)
		}

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, nil)

		assertPackageMapping(t, mapping, filepath.Join(virtualPkgDir, "dist", "index.d.ts"), filepath.Join(virtualPkgDir, "dist", "init.luau"))
		assertPackageMapping(t, mapping, filepath.Join(packageRealPath(realPkgDir), "dist", "index.d.ts"), filepath.Join(virtualPkgDir, "dist", "init.luau"))
	})

	t.Run("adds a real path pattern for a symlinked package", func(t *testing.T) {
		dir := t.TempDir()
		realPkgDir := writePackageManifest(t, filepath.Join(dir, "store"), "pkg", `{
			"exports": {
				"./test/*": {"testing": {"types":"./test/*.ts","default":"./out-test/test/*.luau"}}
			}
		}`)
		scopePath := filepath.Join(dir, "node_modules", "@rbxts")
		virtualPkgDir := filepath.Join(scopePath, "pkg")
		if err := os.MkdirAll(scopePath, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(realPkgDir, virtualPkgDir); err != nil {
			t.Skipf("cannot create package symlink: %v", err)
		}

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, []string{"testing"})

		want := filepath.Join(virtualPkgDir, "out-test", "test", "helpers", "styles.luau")
		assertPackageMapping(t, mapping, filepath.Join(virtualPkgDir, "test", "helpers", "styles.ts"), want)
		assertPackageMapping(t, mapping, filepath.Join(packageRealPath(realPkgDir), "test", "helpers", "styles.ts"), want)
	})
}

func TestPackageExportsConditionBranches(t *testing.T) {
	t.Run("reads a custom condition branch for types and runtime", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				"./testing": {
					"testing": {
						"source": "./test/helpers/index.ts",
						"types": "./out-tsc/spec/test/helpers/index.d.ts",
						"default": "./out-test/test/helpers/index.luau"
					}
				}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, []string{"testing"})

		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "out-tsc", "spec", "test", "helpers", "index.d.ts"), filepath.Join(pkgDir, "out-test", "test", "helpers", "index.luau"))
		assertNoPackageMapping(t, mapping, filepath.Join(pkgDir, "test", "helpers", "index.ts"))
	})

	t.Run("skips a custom condition branch when the condition is off", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				"./testing": {
					"testing": {"types": "./out-tsc/x.d.ts", "default": "./out-test/x.luau"}
				}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, nil)

		assertNoPackageMapping(t, mapping, filepath.Join(pkgDir, "out-tsc", "x.d.ts"))
	})

	t.Run("does not take runtime from outside a matched branch", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				"./x": {
					"testing": {"types": "./spec/x.d.ts"},
					"types": "./out/x.d.ts",
					"default": "./out/x.luau"
				}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, []string{"testing"})

		assertNoPackageMapping(t, mapping, filepath.Join(pkgDir, "spec", "x.d.ts"))
		assertNoPackageMapping(t, mapping, filepath.Join(pkgDir, "out", "x.d.ts"))
	})

	t.Run("substitutes a wildcard inside a custom condition branch", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				"./test/*": {
					"testing": {
						"source": "./test/*.ts",
						"types": "./test/*.ts",
						"default": "./out-test/test/*.luau"
					}
				}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, []string{"testing"})

		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "test", "helpers", "styles.ts"), filepath.Join(pkgDir, "out-test", "test", "helpers", "styles.luau"))
		assertNoPackageMapping(t, mapping, filepath.Join(pkgDir, "src", "helpers", "styles.ts"))
	})

	t.Run("prefers the longest wildcard prefix", func(t *testing.T) {
		scopePath := filepath.Join(t.TempDir(), "@rbxts")
		pkgDir := writePackageManifest(t, scopePath, "pkg", `{
			"exports": {
				"./*": {"types": "./out/*.d.ts", "default": "./out/*.luau"},
				"./deep/*": {"types": "./out/deep/*.d.ts", "default": "./shipped/deep/*.luau"}
			}
		}`)

		mapping := createNodeModulesPathMapping([]string{scopePath}, true, nil)

		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "out", "deep", "a.d.ts"), filepath.Join(pkgDir, "shipped", "deep", "a.luau"))
		assertPackageMapping(t, mapping, filepath.Join(pkgDir, "out", "a.d.ts"), filepath.Join(pkgDir, "out", "a.luau"))
	})
}

func writePackageManifest(t *testing.T, scopePath, name, manifest string) string {
	t.Helper()
	pkgDir := filepath.Join(scopePath, name)
	writePackageFile(t, pkgDir, "package.json", manifest)
	return pkgDir
}

func writePackageFile(t *testing.T, pkgDir, name, contents string) {
	t.Helper()
	path := filepath.Join(pkgDir, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}

func assertPackageMapping(t *testing.T, mapping transformer.NodeModulesPathMapping, typesPath, runtimePath string) {
	t.Helper()
	if got, _ := mapping.Lookup(typesPath, true); got != runtimePath {
		t.Errorf("mapping.Lookup(%q) = %q, want %q", typesPath, got, runtimePath)
	}
}

func assertNoPackageMapping(t *testing.T, mapping transformer.NodeModulesPathMapping, typesPath string) {
	t.Helper()
	if got, ok := mapping.Lookup(typesPath, true); ok {
		t.Errorf("mapping.Lookup(%q) = %q, want no mapping", typesPath, got)
	}
}
