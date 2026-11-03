//! `loops_test.go`. The expected text is byte for byte what rbxtsc 3.0.0 emits for the same
//! source (header and trailing `return nil` stripped). `TestOptimizedLoopClosureNoCopies` and
//! `TestLoopClosureCaptureCopies` need function expressions, so they move with slice (ii).

use super::{render_file_statements, render_list, transformer_testdata, with_state};
use crate::statementlist::transform_statement_list;

fn render_control_file(rel_path: &str) -> String {
	with_state(&transformer_testdata("control"), rel_path, render_file_statements)
}

/// A condition with prereqs (`i++ < 3`) is re-evaluated every iteration.
#[test]
fn while_condition_with_prereqs() {
	let want = "local i = 0
while true do
	local _original = i
	i += 1
	if not (_original < 3) then
		break
	end
	print(i)
end
";
	assert_eq!(render_control_file("src/while.ts"), want);
}

/// `for (let i = 0; i < 10; i++)` takes the numeric-for pass, and the bound folds to `9`.
#[test]
fn optimized_numeric_for_literal_bound() {
	let want = "local total = 0
for i = 0, 9 do
	total += i
end
print(total)
";
	assert_eq!(render_control_file("src/optfor.ts"), want);
}

/// `i !== 5` disqualifies the numeric for: the fallback while loop with `_shouldIncrement`.
#[test]
fn general_for_statement() {
	let want = "do
	local i = 0
	local _shouldIncrement = false
	while true do
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 5) then
			break
		end
		print(i)
	end
end
";
	assert_eq!(render_control_file("src/genfor.ts"), want);
}

/// `return;` is `return nil`, and a free-standing block is `do ... end`. The function body's
/// statement list is transformed directly.
#[test]
fn bare_return_in_nested_block() {
	let got = with_state(&transformer_testdata("control"), "src/return.ts", |s| {
		let declaration = s.source_file.statements().nodes()[0];
		assert!(rotor_tsrs::ast::is_function_declaration(declaration));
		let body = declaration.body().expect("the function has a body");
		let statements = transform_statement_list(s, Some(body), body.statements(), None);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		render_list(s, statements)
	});
	let want = "do
	if value > 0 then
		return nil
	end
	print(\"nested\")
end
print(value)
";
	assert_eq!(got, want);
}

/// `throw "..."` is a plain `error("...")` call statement.
#[test]
fn throw_string() {
	let want = "local bad = math.random() > 2
if bad then
	error(\"something went wrong\")
end
print(\"ok\")
";
	assert_eq!(render_control_file("src/throw.ts"), want);
}

/// A body write to the loop variable triggers the copies, and `continue` gets a clone of the
/// finalizer spliced in front of it.
#[test]
fn loop_copies_continue_finalizer() {
	let want = "local total = 0
do
	local _i = 0
	local _shouldIncrement = false
	while true do
		local i = _i
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 8) then
			break
		end
		if i == 2 then
			i = i + 1
			_i = i
			continue
		end
		total += i
		_i = i
	end
end
print(total)
";
	assert_eq!(render_control_file("src/contfinal.ts"), want);
}

/// The `continue` inside a nested loop targets that loop, so no finalizer is spliced into it.
#[test]
fn loop_finalizer_not_spliced_into_nested_loop() {
	let want = "local hits = 0
do
	local _i = 0
	while true do
		local i = _i
		if not (i ~= 3) then
			break
		end
		i = i + 1
		do
			local j = 0
			local _shouldIncrement = false
			while true do
				if _shouldIncrement then
					j += 1
				else
					_shouldIncrement = true
				end
				if not (j ~= 2) then
					break
				end
				if j == 1 then
					continue
				end
				hits += 1
			end
		end
		_i = i
	end
end
print(hits)
";
	assert_eq!(render_control_file("src/nestedcont.ts"), want);
}

/// A loop variable written in the body with no incrementor still gets the copies.
#[test]
fn loop_write_only_variable_copies() {
	let want = "local calls = 0
do
	local _j = 0
	while true do
		local j = _j
		if not (j ~= 5) then
			break
		end
		j = j + 2
		calls += 1
		_j = j
	end
end
print(calls)
";
	assert_eq!(render_control_file("src/writeonly.ts"), want);
}
