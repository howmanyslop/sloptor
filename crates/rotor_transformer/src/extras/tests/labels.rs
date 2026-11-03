//! `labels_test.go`: loop labels, a rotor extension. Unlike the other expectations, the Luau
//! below is authored, not rbxtsc-derived: rbxtsc 3.0.0 rejects every one of these programs
//! with `noLabeledStatement`.

use super::has_diagnostic;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;
use crate::tests::{render_file_statements, render_list, transformer_testdata, with_state};

fn render_label_fixture(name: &str, want: &str) {
	let got = with_state(
		&transformer_testdata("control"),
		&format!("src/{name}"),
		render_file_statements,
	);
	assert_eq!(got, want, "rendered output differs");
}

/// Transforms a fixture and returns its diagnostics instead of failing on them.
fn label_diagnostics(name: &str) -> Vec<Diagnostic> {
	with_state(&transformer_testdata("control"), &format!("src/{name}"), |state| {
		let file = state.source_file;
		let statements = transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		// exercise the renderer even on the error paths
		let _ = render_list(state, statements);
		state.diags.flush()
	})
}

/// `break outer` from inside a nested loop sets the flag; the check after the inner loop
/// re-breaks.
#[test]
fn label_break_out_of_nested_loop() {
	render_label_fixture(
		"labelbreak.ts",
		r#"local n = 0
local _outer
for _, a in { 1, 2 } do
	for _1, b in { 1, 2 } do
		if b == 2 then
			_outer = "break"
			break
		end
		n += a * b
	end
	if _outer == "break" then
		break
	end
end
print(n)
"#,
	);
}

/// The continue counterpart: the labeled loop resets the flag at the top of every iteration.
#[test]
fn label_continue_out_of_nested_loop() {
	render_label_fixture(
		"labelcontinue.ts",
		r#"local n = 0
local _outer
for _, a in { 1, 2 } do
	_outer = "none"
	for _1, b in { 1, 2 } do
		if b == 2 then
			_outer = "continue"
			break
		end
		n += 1
	end
	if _outer == "continue" then
		continue
	end
	n += 100
end
print(n)
"#,
	);
}

/// A switch lowers to `repeat ... until true`, so it counts as a break boundary.
#[test]
fn label_break_through_switch() {
	render_label_fixture(
		"labelswitch.ts",
		r#"local n = 0
local _outer
for _, a in { 1, 2, 3 } do
	repeat
		if a == 2 then
			_outer = "break"
			break
		end
	until true
	if _outer == "break" then
		break
	end
	n += a
end
print(n)
"#,
	);
}

/// A label on a block becomes `repeat ... until true` with no flag and no nested `do`.
#[test]
fn label_block() {
	render_label_fixture(
		"labelblock.ts",
		"local n = 0
repeat
	n += 1
	if n == 1 then
		break
	end
	n += 100
until true
print(n)
",
	);
}

/// The same wrapper covers any other labeled non-loop statement.
#[test]
fn label_if_statement() {
	render_label_fixture(
		"labelif.ts",
		"local n = 0
repeat
	if n == 0 then
		n += 1
		if n == 1 then
			break
		end
		n += 100
	end
until true
print(n)
",
	);
}

/// `a: b: for` attaches both labels to the same loop; the continue check covers both.
#[test]
fn label_multiple_on_one_loop() {
	render_label_fixture(
		"labelmulti.ts",
		r#"local n = 0
local _a
local _b
for _, x in { 1, 2, 3 } do
	_a = "none"
	_b = "none"
	for _1, y in { 1, 2 } do
		if y == 1 then
			_a = "continue"
			break
		end
		if x == 3 then
			_b = "continue"
			break
		end
		n += 1
	end
	if _a == "continue" or _b == "continue" then
		continue
	end
	n += 100
end
print(n)
"#,
	);
}

/// The check exists only on boundaries strictly inside the label's owner, so an unlabeled
/// outer loop keeps running.
#[test]
fn label_unlabeled_outer_loop() {
	render_label_fixture(
		"labelouter.ts",
		r#"local n = 0
for _, x in { 1, 2 } do
	local _a
	for _1, y in { 1, 2 } do
		for _2, z in { 1, 2 } do
			if z == 2 then
				_a = "break"
				break
			end
			n += 1
		end
		if _a == "break" then
			break
		end
	end
	n += 100
end
print(n)
"#,
	);
}

/// A loop inside a closure inside a labeled loop does not inherit the label state.
#[test]
fn label_function_barrier() {
	render_label_fixture(
		"labelclosure.ts",
		r#"local n = 0
local _outer
for _, a in { 1, 2 } do
	local f = function()
		for _1, b in { 1, 2 } do
			n += b
		end
	end
	f()
	for _1, c in { 1, 2 } do
		if c == 2 then
			_outer = "break"
			break
		end
	end
	if _outer == "break" then
		break
	end
end
print(n)
"#,
	);
}

/// The try lowering emits Luau functions, so it is a barrier too; a label inside the try body
/// is fine.
#[test]
fn label_try_barrier() {
	render_label_fixture(
		"labeltryinner.ts",
		r#"local n = 0
for _, x in { 1, 2 } do
	TS.try(function()
		local _inner
		for _1, y in { 1, 2 } do
			for _2, z in { 1, 2 } do
				if z == 2 then
					_inner = "break"
					break
				end
				n += 1
			end
			if _inner == "break" then
				break
			end
		end
	end, function(e)
		n += 1
	end)
end
print(n)
"#,
	);
}

/// A label nothing targets costs nothing.
#[test]
fn label_unused() {
	render_label_fixture(
		"labelunused.ts",
		"local n = 0
for _, a in { 1, 2 } do
	n += a
end
print(n)
",
	);
}

/// The resets land above the loop body, also above any hoisted condition prereqs.
#[test]
fn label_while_statement() {
	render_label_fixture(
		"labelwhile.ts",
		r#"local n = 0
local _outer
while n < 10 do
	_outer = "none"
	for _, a in { 1, 2 } do
		if a == 2 then
			_outer = "continue"
			break
		end
		n += 1
	end
	if _outer == "continue" then
		continue
	end
	n += 100
end
print(n)
"#,
	);
}

/// `do ... while` lowers to `repeat` with an inner `do ... end`; the check lands inside it.
#[test]
fn label_do_statement() {
	render_label_fixture(
		"labeldo.ts",
		r#"local n = 0
local _outer
repeat
	do
		for _, a in { 1, 2 } do
			if a == 2 then
				_outer = "break"
				break
			end
			n += 1
		end
		if _outer == "break" then
			break
		end
	end
until not (n < 10)
print(n)
"#,
	);
}

/// The numeric-for fast path keeps its shape; the reset is unshifted into its body.
#[test]
fn label_optimized_for_statement() {
	render_label_fixture(
		"labeloptfor.ts",
		r#"local n = 0
local _outer
for i = 0, 2 do
	_outer = "none"
	for j = 0, 2 do
		if j == 2 then
			_outer = "continue"
			break
		end
		n += 1
	end
	if _outer == "continue" then
		continue
	end
	n += 100
end
print(n)
"#,
	);
}

/// The loop-carried write-back is spliced before the generated `continue`; the reset sits
/// above the `_shouldIncrement` guard.
#[test]
fn label_fallback_for_statement() {
	render_label_fixture(
		"labelfallbackfor.ts",
		r#"local fns = {}
local _outer
do
	local _i = 0
	local _shouldIncrement = false
	while true do
		_outer = "none"
		local i = _i
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 3) then
			break
		end
		table.insert(fns, function()
			return i
		end)
		for _, a in { 1, 2 } do
			if a == 2 then
				_outer = "continue"
				break
			end
		end
		if _outer == "continue" then
			_i = i
			continue
		end
		_i = i
	end
end
print(#fns)
"#,
	);
}

/// `$range` takes its own numeric-for path and shares the one break scope.
#[test]
fn label_range_macro() {
	render_label_fixture(
		"labelrange.ts",
		r#"local n = 0
local _outer
for i = 1, 3 do
	for j = 1, 3 do
		if j == 2 then
			_outer = "break"
			break
		end
		n += i
	end
	if _outer == "break" then
		break
	end
end
print(n)
"#,
	);
}

/// The inline `[k, v]` Map fast path unshifts its own bindings into the same body list.
#[test]
fn label_for_of_destructuring() {
	render_label_fixture(
		"labeldestructure.ts",
		r#"local n = 0
local map = {}
local _outer
for k, v in map do
	for _, w in { 1, 2 } do
		if w == 2 then
			_outer = "break"
			break
		end
		n += v
	end
	if _outer == "break" then
		break
	end
end
print(n)
"#,
	);
}

/// A label whose target lies outside an enclosing try is rejected.
#[test]
fn label_within_try_catch_diagnostic() {
	for name in ["labeltrybreak.ts", "labeltrycontinue.ts"] {
		let diagnostics = label_diagnostics(name);
		assert!(
			has_diagnostic(
				&diagnostics,
				"noLabeledStatementsWithinTryCatch",
				"labels are not supported within try/catch blocks!"
			),
			"{name}: expected noLabeledStatementsWithinTryCatch, got {diagnostics:?}"
		);
	}
}

/// An undefined label (only reachable behind `// @ts-ignore`) fails loudly.
#[test]
fn label_unknown_diagnostic() {
	let diagnostics = label_diagnostics("labelunknown.ts");
	assert!(
		has_diagnostic(&diagnostics, "rotorUnknownLabel", "no enclosing label named `nope`"),
		"expected rotorUnknownLabel, got {diagnostics:?}"
	);
}

/// An unlabeled `break`/`continue` the `repeat ... until true` wrapper would capture is
/// rejected rather than miscompiled.
#[test]
fn label_captured_flow_control_diagnostic() {
	let diagnostics = label_diagnostics("labelcapture.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"rotorLabeledStatementFlowControl",
			"cannot contain an unlabeled"
		),
		"expected rotorLabeledStatementFlowControl, got {diagnostics:?}"
	);
}

/// A labeled `continue` cannot target a do/while whose condition needs temps.
#[test]
fn label_continue_in_do_while_diagnostic() {
	let diagnostics = label_diagnostics("labeldocond.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"rotorLabeledContinueInDoWhile",
			"cannot target a do/while loop"
		),
		"expected rotorLabeledContinueInDoWhile, got {diagnostics:?}"
	);
}

/// The pre-existing rbxtsc hole: an unlabeled `continue` in a do/while with a complex
/// condition emits a `continue` ahead of `local _condition`. Byte parity outranks correctness.
#[test]
fn plain_continue_in_do_while_unchanged() {
	render_label_fixture(
		"labelplaindocond.ts",
		"local n = 0
local function a()
	n += 1
	return n < 5
end
repeat
	do
		if n == 2 then
			continue
		end
		n += 1
	end
	local _condition = a()
	if _condition then
		local _original = n
		n += 1
		_condition = _original < 20
	end
until not _condition
print(n)
",
	);
}
