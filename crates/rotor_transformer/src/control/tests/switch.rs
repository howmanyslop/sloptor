//! `switch_test.go`. The expected text in the byte-parity tests is byte for byte what rbxtsc
//! 3.0.0 emits for the same source, minus the version header and the module tail.

use std::process::Command;

use super::render_file;

/// a case expression with prereqs (`i++`) reachable by fallthrough takes the guarded variant — the
/// prereqs run only when not already falling through, the flag takes over the comparison
/// (`_fallthrough = _exp == _original`), and the clause condition collapses to just
/// `_fallthrough`. The preceding empty case emits only the flag update. Operand capture follows
/// 3.0.0-dev-3106b14 and is covered by the shared runtime fixture.
#[test]
fn switch_case_prereqs_fallthrough() {
	let want = "local i = 10
local function pick(n)
	local _exp = n
	repeat
		local _fallthrough = false
		if _exp == 0 then
			_fallthrough = true
		end
		if not _fallthrough then
			local _original = i
			i += 1
			_fallthrough = _exp == _original
		end
		if _fallthrough then
			return \"low\"
		end
		return \"high\"
	until true
end
print(pick(0), i)
";
	assert_eq!(render_file("switch", "src/prereq.ts"), want);
}

/// the checkVariableHoist end-to-end proof — a case-local declared in case 0 and written from case
/// 1 hoists to `local shared` BEFORE case 0's `if` (keyed on the `CaseClause`, emitted by
/// createHoistDeclaration inside transformCaseClause), and the declaration inside the clause
/// demotes to an assignment. No `_fallthrough` is declared: both bodies end in `break`.
#[test]
fn switch_cross_clause_hoist() {
	let want = "local function describe(n)
	local result = 0
	repeat
		local shared
		if n == 0 then
			shared = n + 1
			result = shared
			break
		end
		if n == 1 then
			shared = n + 2
			result = shared
			break
		end
	until true
	return result
end
print(describe(0), describe(1))
";
	assert_eq!(render_file("switch", "src/casehoist.ts"), want);
}

/// TS break-in-switch emits a plain Luau `break` that exits the `repeat ... until true` — the
/// enclosing numeric for is untouched. The complex subject (`i % 3`) gets `local _exp` as a prereq
/// before the repeat. PORT NOTE quirk: the TS `continue` (which targets the LOOP in TS) emits a
/// Luau `continue` inside the repeat, where it jumps to the repeat's `until` and acts like a
/// switch-break instead — `total += 100` still runs. Upstream emits exactly this; pinned byte-for-
/// byte.
#[test]
fn switch_inside_loop() {
	let want = "local total = 0
for i = 0, 4 do
	local _exp = i % 3
	repeat
		if _exp == 0 then
			total += 1
			break
		end
		if _exp == 1 then
			continue
		end
		total += 10
	until true
	total += 100
end
print(total)
";
	assert_eq!(render_file("switch", "src/inloop.ts"), want);
}

/// each switch is its own repeat; the inner `break` exits the inner repeat, the outer clause's
/// trailing `break` exits the outer one.
#[test]
fn nested_switch() {
	let want = "local function grid(a, b)
	local r = 0
	repeat
		if a == 1 then
			repeat
				if b == 1 then
					r = 11
					break
				end
				r = 19
			until true
			break
		end
		r = 99
	until true
	return r
end
print(grid(1, 1), grid(1, 5), grid(2, 0))
";
	assert_eq!(render_file("switch", "src/nested.ts"), want);
}

/// This output regression also runs when the optional Luau CLI is absent.
#[test]
fn switch_emits_clauses_after_default() {
	let output = render_file("switch", "src/afterdefault.ts");
	for want in ["y = 2", "y = 3"] {
		assert!(output.contains(want), "missing {want:?}:\n{output}");
	}
}

/// Cases after default must still match, and default must fall through. Skipped without the
/// `luau` CLI on the path, as in Go.
#[test]
fn switch_clauses_after_default() {
	if Command::new("luau").arg("--help").output().is_err() {
		return;
	}
	for (file, want) in [
		("src/afterdefault.ts", "2"),
		("src/defaultedges.ts", "12\t12\t123\n2\t3\n1\t8\n11\t20\n11"),
		("src/defaultselection.ts", "1\t2\t0\n12\t2\t12\n0\t2\t1"),
	] {
		let output = render_file("switch", file);
		let directory = tempfile::tempdir().expect("a temporary directory");
		let script = directory.path().join("switch.luau");
		std::fs::write(&script, &output).expect("the script writes");
		let result = Command::new("luau").arg(&script).output().expect("luau runs");
		let mut got = String::from_utf8_lossy(&result.stdout).into_owned();
		got.push_str(&String::from_utf8_lossy(&result.stderr));
		assert!(result.status.success(), "Luau failed:\n{got}\n{output}");
		assert_eq!(got.replace("\r\n", "\n").trim(), want, "{file}\n{output}");
	}
}
