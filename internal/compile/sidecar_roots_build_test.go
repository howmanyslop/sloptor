package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ambientProbePlugin reports what the checker thinks `ambient` is typed as,
// inlining the answer in place of the AMBIENT_TYPE marker. The answer depends
// on whether the file that declares the global reached the worker's program, so
// the emitted Luau is a direct readout of the root set the worker was given.
const ambientProbePlugin = `const ts = require("typescript");

module.exports = function ambientProbe(program) {
	const checker = program.getTypeChecker();
	return (context) => (sourceFile) => {
		const answerFor = (file) => {
			for (const statement of file.statements) {
				if (!ts.isVariableStatement(statement)) {
					continue;
				}
				for (const declaration of statement.declarationList.declarations) {
					if (ts.isIdentifier(declaration.name) && declaration.name.text === "ambient") {
						// The members, not the type NAME: TypeScript keeps the
						// written name for an unresolved type reference, so
						// typeToString would answer the same either way.
						const members = checker.getPropertiesOfType(checker.getTypeAtLocation(declaration.name));
						return members.length === 0 ? "unresolved" : members.map((symbol) => symbol.name).sort().join("+");
					}
				}
			}
			return "no-declaration";
		};
		const visit = (node) => {
			if (ts.isStringLiteral(node) && node.text === "AMBIENT_TYPE") {
				return ts.factory.createStringLiteral(answerFor(sourceFile));
			}
			return ts.visitEachChild(node, visit, context);
		};
		return ts.visitNode(sourceFile, visit);
	};
};
`

// A `declare global` lives in a module nothing imports, so it only reaches the
// worker's checker if the narrowed root set keeps that module. This is the
// soundness case for narrowedSidecarRoots: an incremental rebuild has to give a
// transformer the same checker answers a full build does.
func TestNarrowedSidecarRootsPreserveGlobalAugmentationsForTransformers(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()

	dir := writeProject(t, "@scope/narrow-roots-ambient", "")
	// Registered after the fixture's t.TempDir so it runs before that cleanup:
	// Windows refuses to delete files the worker still has open.
	t.Cleanup(closeSidecarSessions)
	writeSidecarPluginFixture(t, dir, "", sidecarDeclarationConfig(`[{"transform":"./plugins/ambient-probe.js"}]`))
	enableDeclarationIncrementalBuilds(t, dir)
	if err := os.WriteFile(filepath.Join(dir, "plugins", "ambient-probe.js"), []byte(ambientProbePlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	for rel, text := range map[string]string{
		// Imported by nobody, yet its `declare global` types `ambient` below.
		"src/augment.ts": "export {};\ndeclare global {\n\tinterface RotorAmbient {\n\t\tname: string;\n\t}\n}\n",
		"src/main.ts":    "declare const ambient: RotorAmbient;\nexport const probe = \"AMBIENT_TYPE\";\nexport const named = ambient.name;\n",
		"src/filler.ts":  "export const filler = 1;\n",
		"src/filler2.ts": "export const filler2 = 2;\n",
	} {
		if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(rel)), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	probe := func(stage string) string {
		t.Helper()
		timings := NewBuildTimings()
		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil {
			t.Fatalf("%s build: %v (diags: %v)", stage, err, diags)
		}
		timings.finish()
		out, err := os.ReadFile(filepath.Join(dir, "out", "main.luau"))
		if err != nil {
			t.Fatalf("%s: %v", stage, err)
		}
		for _, line := range strings.Split(string(out), "\n") {
			if strings.HasPrefix(strings.TrimSpace(line), "local probe =") {
				return strings.TrimSpace(line)
			}
		}
		t.Fatalf("%s build emitted no probe line:\n%s", stage, out)
		return ""
	}

	full := probe("full")
	if full != `local probe = "name"` {
		t.Fatalf("full build resolved the ambient type as %s", full)
	}

	// A warm worker keeps whatever root set the full build left it, so the
	// narrowed request only reaches a session that did not serve that full
	// build — which is every `sloptor build` after the first, each a fresh
	// process.
	closeSidecarSessions()

	// A comment-only edit selects main.ts alone, which is what narrows the
	// worker's root set.
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"),
		[]byte("declare const ambient: RotorAmbient;\nexport const probe = \"AMBIENT_TYPE\";\nexport const named = ambient.name;\n// edited\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	incremental := probe("incremental")
	if incremental != full {
		t.Fatalf("the narrowed rebuild changed the transformer's checker answer: full %s, incremental %s", full, incremental)
	}
}
