//! `conditional_test.go`: the `transformConditionalExpression` paths the diff fixtures don't
//! isolate (statement position, branch prereqs, a TS-truthy condition, and a conditional inside a
//! for incrementor). The expected text is byte-for-byte what rbxtsc 3.0.0 emits.

use super::{render_file_statements, transformer_testdata, with_state};

#[test]
fn conditional_against_rbxtsc() {
	with_state(&transformer_testdata("conditional"), "src/conditional.ts", |s| {
		let want = r#"local i = 5
if flag then
	print("a")
else
	print("b")
end
local _result
if flag then
	local _original = i
	i += 1
	_result = _original
else
	_result = 0
end
local x = _result
local y = if n ~= 0 and n == n and n then 1 else 2
do
	local j = 0
	local _shouldIncrement = false
	while true do
		if _shouldIncrement then
			j = if flag then j + 1 else j + 2
		else
			_shouldIncrement = true
		end
		if not (j < 3) then
			break
		end
		print(j)
	end
end
print(i, x, y)
"#;
		assert_eq!(render_file_statements(s), want);
	});
}
