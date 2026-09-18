package compile

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestNativeFlameworkGuardPreservesScopedWorkspaceImport(t *testing.T) {
	for _, localGuard := range []bool{false, true} {
		name := "hoisted_guard_dependency"
		if localGuard {
			name = "local_guard_dependency"
		}
		t.Run(name, func(t *testing.T) {
			workspace := t.TempDir()
			dir := filepath.Join(workspace, "packages", "consumer")
			writeFile(t, dir, "package.json", `{"name":"@workspace/consumer"}`)
			writeFile(t, dir, "rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n")
			writeFile(t, dir, "tsconfig.json", `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@workspace", "node_modules/@rbxts", "node_modules/@flamework"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`)
			writeFile(t, dir, "src/globals.d.ts", strings.Replace(noLibGlobalStubs, "interface Array<T> {}", "interface Array<T> { readonly length: number; readonly [index: number]: T; }", 1))

			core := filepath.Join(workspace, "node_modules", ".pnpm", "@flamework+core@1.0.0", "node_modules", "@flamework", "core")
			guard := filepath.Join(workspace, "node_modules", ".pnpm", "@rbxts+t@1.0.0", "node_modules", "@rbxts", "t")
			provider := filepath.Join(workspace, "packages", "provider")
			writeFile(t, core, "package.json", `{"name":"@flamework/core","version":"1.0.0","types":"out/index.d.ts","main":"out/init.luau"}`)
			writeFile(t, core, "out/index.d.ts", `import { t } from "@rbxts/t";
export declare namespace Flamework {
	/** @metadata macro */
	function createGuard<T>(meta?: { _flamework_macro_generic: [T, "guard"] }): t.check<T>;
}`)
			writeFile(t, core, "out/init.luau", "return { Flamework = { createGuard = function(guard) return guard end } }\n")
			writeFile(t, core, "out/prelude.d.ts", "export { t } from \"@rbxts/t\";\n")
			writeFile(t, core, "out/prelude.luau", "local TS = _G[script]\nreturn { t = TS.import(script, TS.getModule(script, \"@rbxts\", \"t\").lib).t }\n")
			writeFile(t, guard, "package.json", `{"name":"@rbxts/t","version":"1.0.0","types":"lib/index.d.ts","main":"lib/init.luau"}`)
			writeFile(t, guard, "lib/index.d.ts", "export declare namespace t {\n\ttype check<T> = (value: unknown) => value is T;\n\tfunction string(value: unknown): value is string;\n}\n")
			writeFile(t, guard, "lib/init.luau", "return { t = { string = function(value) return type(value) == \"string\" end } }\n")
			writeFile(t, provider, "package.json", `{"name":"@workspace/provider","version":"1.0.0","types":"src/index.d.ts","main":"src/init.luau","exports":{".":{"types":"./src/index.d.ts","default":"./src/init.luau"}}}`)
			writeFile(t, provider, "src/index.d.ts", "export declare const value: number;\n")
			writeFile(t, provider, "src/init.luau", "return { value = 1 }\n")
			links := map[string]string{
				filepath.Join(dir, "node_modules", "@workspace", "provider"): provider,
				filepath.Join(dir, "node_modules", "@flamework", "core"):     core,
				filepath.Join(workspace, "node_modules", "@rbxts", "t"):      guard,
				filepath.Join(core, "..", "..", "@rbxts", "t"):               guard,
			}
			if localGuard {
				links[filepath.Join(dir, "node_modules", "@rbxts", "t")] = guard
			}
			for link, target := range links {
				if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
					t.Fatal(err)
				}
				if runtime.GOOS == "windows" {
					if output, err := exec.Command("cmd", "/c", "mklink", "/J", link, target).CombinedOutput(); err != nil {
						t.Fatalf("mklink /J: %v: %s", err, output)
					}
				} else if err := os.Symlink(target, link); err != nil {
					t.Fatal(err)
				}
			}

			for _, withGuard := range []bool{false, true} {
				name := "without_guard"
				main := "import { value } from \"@workspace/provider\";\nimport { Flamework } from \"@flamework/core\";\nexport const result = value;\n"
				if withGuard {
					name = "with_guard"
					main += "export const isString: (value: unknown) => value is string = Flamework.createGuard<string>();\n"
				}
				t.Run(name, func(t *testing.T) {
					writeFile(t, dir, "src/main.ts", main)
					result, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})
					if err != nil || len(diagnostics) != 0 {
						t.Fatalf("BuildProjectWithOptions: %v (%v)", err, diagnostics)
					}
					output := result.Outputs["out/main.luau"]
					if !strings.Contains(output, `TS.import(script, TS.getModule(script, "@workspace", "provider").src).value`) {
						t.Fatalf("workspace runtime import missing:\n%s", output)
					}
					if withGuard {
						guardImport := `TS.import(script, TS.getModule(script, "@flamework", "core").out.prelude).t`
						if localGuard {
							guardImport = `TS.import(script, TS.getModule(script, "@rbxts", "t").lib).t`
						}
						if !strings.Contains(output, guardImport) || !strings.Contains(output, "t.string") {
							t.Fatalf("generated guard runtime import missing:\n%s", output)
						}
					}
				})
			}
		})
	}
}

func TestNativeFlameworkRejectsUnsupportedRuntimeImports(t *testing.T) {
	for _, test := range []struct {
		module  string
		message string
	}{
		{"unscoped", "You cannot use modules directly under node_modules."},
		{"@unconfigured/provider", "You can only use npm scopes that are listed in your typeRoots."},
	} {
		t.Run(test.module, func(t *testing.T) {
			dir := writeProject(t, "@workspace/consumer", "")
			writeFile(t, dir, "rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n")
			moduleDir := filepath.Join(dir, "node_modules", filepath.FromSlash(test.module))
			writeFile(t, moduleDir, "package.json", `{"types":"index.d.ts","main":"init.luau"}`)
			writeFile(t, moduleDir, "index.d.ts", "export declare const value: number;\n")
			writeFile(t, moduleDir, "init.luau", "return { value = 1 }\n")
			writeFile(t, dir, "src/main.ts", "import { value } from \""+test.module+"\";\nexport const result = value;\n")
			_, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})
			if err == nil || len(diagnostics) != 1 || diagnostics[0] != test.message {
				t.Fatalf("BuildProjectWithOptions: %v (%v), want %q", err, diagnostics, test.message)
			}
		})
	}
}
