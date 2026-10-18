package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runWizard drives runInitInteractive with a scripted stdin, returning the
// exit code and the rendered output.
func runWizard(t *testing.T, dir, name, script string) (int, string) {
	t.Helper()
	var out bytes.Buffer
	code := runInitInteractive(dir, name, strings.NewReader(script), &out)
	return code, out.String()
}

func mustParseJSON(t *testing.T, path string) map[string]any {
	t.Helper()
	var v map[string]any
	if err := json.Unmarshal([]byte(mustReadFile(t, path)), &v); err != nil {
		t.Fatalf("%s is not valid JSON: %v", filepath.Base(path), err)
	}
	return v
}

// Full flow: game + biome + services/react + asset sync + deploy.
func TestWizardFullFlow(t *testing.T) {
	dir := t.TempDir()
	script := strings.Join([]string{
		"my-game", // project name
		"1",       // template: game
		"1",       // linter: biome
		"1,2",     // packages: services + react/react-roblox
		"y",       // set up asset sync?
		"",        // assets directory (assets)
		"1",       // creator: user
		"123456",  // creator id
		"y",       // set up deploy environments?
		"",        // environment name (production)
		"111",     // universe id
		"222",     // place id
		"",        // place file (build/game.rbxl)
		"y",       // create?
	}, "\n") + "\n"

	code, out := runWizard(t, dir, "fallback-name", script)
	if code != 0 {
		t.Fatalf("exit code = %d\noutput:\n%s", code, out)
	}

	for _, f := range []string{
		"package.json", "tsconfig.json", "default.project.json", "biome.json",
		"rotor.toml", "assets/.gitkeep", "include/.gitkeep",
		"src/shared/module.ts", "src/server/main.server.ts", "src/client/main.client.ts",
		"rotor.d.ts",
	} {
		if !fileExists(filepath.Join(dir, f)) {
			t.Errorf("missing scaffolded file %s", f)
		}
	}

	// package.json: wizard name, linter scripts/devDep, selected dependencies.
	pkg := mustParseJSON(t, filepath.Join(dir, "package.json"))
	if pkg["name"] != "my-game" {
		t.Errorf("package.json name = %v, want my-game", pkg["name"])
	}
	scripts, _ := pkg["scripts"].(map[string]any)
	if scripts["lint"] != "biome check src" || scripts["format"] != "biome format --write src" {
		t.Errorf("biome scripts wrong: %v", scripts)
	}
	devDeps, _ := pkg["devDependencies"].(map[string]any)
	if v, _ := devDeps["@biomejs/biome"].(string); !strings.HasPrefix(v, "^") {
		t.Errorf("devDependencies[@biomejs/biome] = %q, want a loose ^ pin", v)
	}
	deps, _ := pkg["dependencies"].(map[string]any)
	for _, d := range []string{"@rbxts/services", "@rbxts/react", "@rbxts/react-roblox"} {
		if _, ok := deps[d]; !ok {
			t.Errorf("dependencies missing %s (got %v)", d, deps)
		}
	}
	if deps["@rbxts/services"] != "^1.0.0" {
		t.Errorf("dependencies[@rbxts/services] = %v, want ^1.0.0", deps["@rbxts/services"])
	}

	// tsconfig: jsx options uncommented because react was selected.
	tsconfig := mustReadFile(t, filepath.Join(dir, "tsconfig.json"))
	if strings.Contains(tsconfig, `// "jsx"`) || !strings.Contains(tsconfig, "\t\t\"jsx\": \"react\",") {
		t.Errorf("tsconfig jsx should be uncommented for react:\n%s", tsconfig)
	}
	for _, want := range []string{`"jsxFactory": "React.createElement"`, `"jsxFragmentFactory": "React.Fragment"`} {
		if !strings.Contains(tsconfig, want) {
			t.Errorf("tsconfig missing %s", want)
		}
	}
	var tsc map[string]any
	if err := json.Unmarshal([]byte(stripJSONCComments(tsconfig)), &tsc); err != nil {
		t.Fatalf("tsconfig.json (comments stripped) is not valid JSON: %v", err)
	}

	// rotor.toml: real (uncommented) assets + deploy sections.
	cfg := mustReadFile(t, filepath.Join(dir, "rotor.toml"))
	for _, want := range []string{
		"#:schema https://raw.githubusercontent.com/howmanyslop/sloptor",
		"[assets]",
		`paths = ["assets/**/*.png", "assets/**/*.ogg"]`,
		"[assets.creator]",
		`type = "user"`,
		"id = 123456",
		"[deploy.environments.production]",
		"universeId = 111",
		"[deploy.environments.production.places.start]",
		`file = "build/game.rbxl"`,
		"placeId = 222",
	} {
		if !strings.Contains(cfg, want) {
			t.Errorf("rotor.toml missing %q:\n%s", want, cfg)
		}
	}
	if strings.Contains(cfg, "# [assets]") || strings.Contains(cfg, "# [deploy.") {
		t.Error("rotor.toml should not keep commented skeletons for configured sections")
	}

	// Every emitted JSON file parses.
	mustParseJSON(t, filepath.Join(dir, "biome.json"))
	mustParseJSON(t, filepath.Join(dir, "default.project.json"))

	// The summary listed the files before writing.
	if !strings.Contains(out, "summary") || !strings.Contains(out, "biome.json") {
		t.Errorf("wizard output missing summary block:\n%s", out)
	}
}

// Defaults everywhere: enter through every prompt → game + biome, no extras,
// commented config skeleton, confirm defaults to yes.
func TestWizardDefaults(t *testing.T) {
	dir := t.TempDir()
	script := strings.Repeat("\n", 7) // name, template, linter, packages, assets?, deploy?, create?

	code, out := runWizard(t, dir, "proj", script)
	if code != 0 {
		t.Fatalf("exit code = %d\noutput:\n%s", code, out)
	}
	pkg := mustParseJSON(t, filepath.Join(dir, "package.json"))
	if pkg["name"] != "proj" {
		t.Errorf("package.json name = %v, want proj", pkg["name"])
	}
	if _, ok := pkg["dependencies"]; ok {
		t.Error("defaults run should not add extra dependencies")
	}
	if !fileExists(filepath.Join(dir, "biome.json")) {
		t.Error("wizard default linter is biome; biome.json missing")
	}
	cfg := mustReadFile(t, filepath.Join(dir, "rotor.toml"))
	if !strings.Contains(cfg, "# [assets]") || !strings.Contains(cfg, "# [deploy.environments.dev]") {
		t.Error("skipped cloud sections should keep the commented skeleton")
	}
	if fileExists(filepath.Join(dir, "assets")) {
		t.Error("assets dir should not be created when asset sync is skipped")
	}
	tsconfig := mustReadFile(t, filepath.Join(dir, "tsconfig.json"))
	if !strings.Contains(tsconfig, `// "jsx": "react"`) {
		t.Error("tsconfig jsx should stay commented without react")
	}
}

// Abort at the summary: exit 0, nothing written.
func TestWizardAbort(t *testing.T) {
	dir := t.TempDir()
	script := strings.Repeat("\n", 6) + "n\n"

	code, out := runWizard(t, dir, "proj", script)
	if code != 0 {
		t.Fatalf("exit code = %d, want 0", code)
	}
	if !strings.Contains(out, "aborted, nothing written") {
		t.Errorf("output missing abort message:\n%s", out)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Errorf("abort must write nothing, found %d entries", len(entries))
	}
}

// Invalid menu/yes-no input re-prompts with a hint instead of failing.
func TestWizardInvalidInputReprompts(t *testing.T) {
	dir := t.TempDir()
	script := strings.Join([]string{
		"proj",  // name
		"9",     // template: out of range → re-prompt
		"abc",   // template: not a number → re-prompt
		"2",     // template: package
		"5",     // linter: out of range → re-prompt
		"3",     // linter: none
		"0",     // packages: out of range → re-prompt
		"",      // packages: none
		"maybe", // asset sync: invalid → re-prompt
		"n",     // asset sync: no
		"n",     // deploy: no
		"y",     // create
	}, "\n") + "\n"

	code, out := runWizard(t, dir, "proj", script)
	if code != 0 {
		t.Fatalf("exit code = %d\noutput:\n%s", code, out)
	}
	if !strings.Contains(out, "enter a number from 1 to 3") {
		t.Errorf("missing menu re-prompt hint:\n%s", out)
	}
	if !strings.Contains(out, "answer y or n") {
		t.Errorf("missing yes/no re-prompt hint:\n%s", out)
	}
	if !fileExists(filepath.Join(dir, "src/init.ts")) {
		t.Error("package template not scaffolded")
	}
	if fileExists(filepath.Join(dir, "biome.json")) || fileExists(filepath.Join(dir, ".oxlintrc.json")) {
		t.Error("linter none must not write linter configs")
	}
}

// oxlint choice writes .oxlintrc.json + lint script, no biome bits.
func TestWizardOxlint(t *testing.T) {
	dir := t.TempDir()
	script := strings.Join([]string{
		"",  // name default
		"",  // template default (game)
		"2", // linter: oxlint
		"",  // packages: none
		"n", // assets
		"n", // deploy
		"y", // create
	}, "\n") + "\n"

	code, out := runWizard(t, dir, "proj", script)
	if code != 0 {
		t.Fatalf("exit code = %d\noutput:\n%s", code, out)
	}
	mustParseJSON(t, filepath.Join(dir, ".oxlintrc.json"))
	if fileExists(filepath.Join(dir, "biome.json")) {
		t.Error("oxlint choice must not write biome.json")
	}
	pkg := mustParseJSON(t, filepath.Join(dir, "package.json"))
	scripts, _ := pkg["scripts"].(map[string]any)
	if scripts["lint"] != "oxlint src" {
		t.Errorf("scripts[lint] = %v, want oxlint src", scripts["lint"])
	}
	if _, ok := scripts["format"]; ok {
		t.Error("oxlint choice must not add a format script")
	}
	devDeps, _ := pkg["devDependencies"].(map[string]any)
	if _, ok := devDeps["oxlint"]; !ok {
		t.Error("devDependencies missing oxlint")
	}
}

// Input ending mid-wizard is an error exit, not a half-written project.
func TestWizardInputClosed(t *testing.T) {
	dir := t.TempDir()
	code, _ := runWizard(t, dir, "proj", "my-game\n") // stops after the name
	if code != 1 {
		t.Errorf("exit code = %d, want 1 when input closes early", code)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Errorf("early EOF must write nothing, found %d entries", len(entries))
	}
}

// --yes bypasses the wizard and produces the plain non-interactive scaffold.
func TestCmdInitYesFlag(t *testing.T) {
	dir := t.TempDir()
	if code := cmdInit([]string{dir, "--yes"}); code != 0 {
		t.Fatalf("cmdInit exit code = %d", code)
	}
	if !fileExists(filepath.Join(dir, "package.json")) {
		t.Error("--yes should scaffold the default game template")
	}
	if fileExists(filepath.Join(dir, "biome.json")) {
		t.Error("--yes (non-interactive defaults) must not add a linter")
	}
	cfg := mustReadFile(t, filepath.Join(dir, "rotor.toml"))
	if !strings.Contains(cfg, "# [assets]") || !strings.Contains(cfg, "# [deploy.environments.dev]") {
		t.Error("--yes should keep the commented rotor.toml skeleton")
	}
}

// Plain template via the wizard skips the rbxts-only steps entirely.
func TestWizardPlainSkipsRbxtsSteps(t *testing.T) {
	dir := t.TempDir()
	script := strings.Join([]string{
		"",  // name default
		"3", // template: plain
		"y", // create
	}, "\n") + "\n"

	code, out := runWizard(t, dir, "proj", script)
	if code != 0 {
		t.Fatalf("exit code = %d\noutput:\n%s", code, out)
	}
	for _, f := range []string{"default.project.json", "src/init.luau", "aftman.toml"} {
		if !fileExists(filepath.Join(dir, f)) {
			t.Errorf("missing scaffolded file %s", f)
		}
	}
	for _, f := range []string{"package.json", "tsconfig.json", "rotor.toml", "biome.json"} {
		if fileExists(filepath.Join(dir, f)) {
			t.Errorf("plain template should not write %s", f)
		}
	}
	if strings.Contains(out, "linter") {
		t.Errorf("plain template should not ask the linter step:\n%s", out)
	}
}
