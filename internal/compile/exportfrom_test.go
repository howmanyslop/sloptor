package compile

import (
	"testing"
)

// ----------------------------------------------------------------------------
// Export-from declarations + export assignment (Phase 3a Task 5).
//
// Ground truth: every `want` below is VERBATIM rbxtsc 3.0.0 output, captured
// 2026-06-06 by compiling the same _scratch_* sources through
// testdata/diff/project (real toolchain in node_modules) with
// `rbxtsc --type model` and the checked-in default.project.json. Scratch
// sources were deleted afterwards; the testdata/exportfrom_model fixture
// reproduces the setup self-contained (print declared ambiently in
// globals.d.ts, which changes no output bytes). The _scratch_remix case is
// also the digest §2.4 verbatim star-export-ordering ground truth.
// ----------------------------------------------------------------------------

func TestCompileProjectExportFromModel(t *testing.T) {
	files := compileRuntimeLibProject(t, "exportfrom_model")

	header := "-- Compiled with sloptor v2.3.3\n" +
		"local TS = require(script.Parent.include.RuntimeLib)\n"
	wants := map[string]string{
		// the re-exported module itself.
		"out/_scratch_util.luau": "-- Compiled with sloptor v2.3.3\n" +
			"local VALUE = 123\n" +
			"local function greet(name)\n" +
			"\treturn `Hello, {name}`\n" +
			"end\n" +
			"local function default()\n" +
			"\treturn VALUE\n" +
			"end\n" +
			"return {\n" +
			"\tgreet = greet,\n" +
			"\tdefault = default,\n" +
			"\tVALUE = VALUE,\n" +
			"}\n",
		// named re-export, one specifier: TS.import inlined in the
		// statement-position exports assignment; exports-table shape forced.
		"out/_scratch_renamed.luau": header +
			"local exports = {}\n" +
			"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
			"return exports\n",
		// aliased re-export: exports.<name> = importExp.<propertyName>.
		"out/_scratch_realias.luau": header +
			"local exports = {}\n" +
			"exports.hello = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
			"return exports\n",
		// two specifiers: uses > 1 hoists the temp named after the cleaned
		// specifier segment; assignments in specifier order.
		"out/_scratch_retwo.luau": header +
			"local exports = {}\n" +
			"local __scratch_util = TS.import(script, script.Parent, \"_scratch_util\")\n" +
			"exports.greet = __scratch_util.greet\n" +
			"exports.VALUE = __scratch_util.VALUE\n" +
			"return exports\n",
		// star re-export: the `or {}` loop (importExp may be nil in .d.ts).
		"out/_scratch_restar.luau": header +
			"local exports = {}\n" +
			"for _k, _v in TS.import(script, script.Parent, \"_scratch_util\") or {} do\n" +
			"\texports[_k] = _v\n" +
			"end\n" +
			"return exports\n",
		// star + named-from + own export — digest §2.4 VERBATIM ground truth:
		// each export-from statement creates its OWN TS.import (uses counted
		// per declaration); the interleave lives at statement position and the
		// sole exportPairs survivor (`own`) trails.
		"out/_scratch_remix.luau": header +
			"local exports = {}\n" +
			"local own = 1\n" +
			"for _k, _v in TS.import(script, script.Parent, \"_scratch_util\") or {} do\n" +
			"\texports[_k] = _v\n" +
			"end\n" +
			"exports.hello = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
			"exports.own = own\n" +
			"return exports\n",
		// export * as ns: exports.<ns> = whole module table.
		"out/_scratch_rens.luau": header +
			"local exports = {}\n" +
			"exports.util = TS.import(script, script.Parent, \"_scratch_util\")\n" +
			"return exports\n",
		// export-from of a module also imported normally: import declaration
		// and export declaration each run their own use count — two separate
		// inline TS.import calls, no shared temp.
		"out/_scratch_reimport.luau": header +
			"local exports = {}\n" +
			"local VALUE = TS.import(script, script.Parent, \"_scratch_util\").VALUE\n" +
			"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
			"print(VALUE)\n" +
			"return exports\n",
		// `export type { ... } from`: whole declaration elided — no import, no
		// exports table, no runtime lib.
		"out/_scratch_retypeonly.luau": "-- Compiled with sloptor v2.3.3\n" +
			"local n = 5\n" +
			"print(n)\n" +
			"return nil\n",
		// per-specifier `type` elision: only the value specifier survives.
		"out/_scratch_respec_typeonly.luau": header +
			"local exports = {}\n" +
			"exports.greet = TS.import(script, script.Parent, \"_scratch_util\").greet\n" +
			"return exports\n",
		// `export =` a table literal as the file's final statement: direct
		// return, no exports local.
		"out/_scratch_eqtable.luau": "-- Compiled with sloptor v2.3.3\n" +
			"return {\n" +
			"\ta = 1,\n" +
			"\tb = \"two\",\n" +
			"}\n",
		// `export =` a function as the final statement: direct `return f`.
		"out/_scratch_eqfunc.luau": "-- Compiled with sloptor v2.3.3\n" +
			"local function f()\n" +
			"\treturn 1\n" +
			"end\n" +
			"return f\n",
		// `export =` NOT the final statement: `local exports = <expr>` at the
		// statement position, handleExports appends `return exports`.
		"out/_scratch_eqmid.luau": "-- Compiled with sloptor v2.3.3\n" +
			"local t = {\n" +
			"\ta = 1,\n" +
			"}\n" +
			"local exports = t\n" +
			"print(t.a)\n" +
			"return exports\n",
	}

	for name, want := range wants {
		if got, ok := files[name]; !ok {
			t.Errorf("%s: missing from CompileProject output (%v)", name, keys(files))
		} else if got != want {
			t.Errorf("%s:\ngot:\n%s\nwant:\n%s", name, got, want)
		}
	}
	if len(files) != len(wants) {
		t.Errorf("produced %d files, want %d (%v)", len(files), len(wants), keys(files))
	}
}
