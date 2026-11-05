//! `operators_test.go`: the expected text is byte-for-byte what rbxtsc 3.0.0 emits for
//! `testdata/operators/src/operators.ts`.

use super::{render_file_statements, transformer_testdata, with_state};

#[test]
fn operators_against_rbxtsc() {
	with_state(&transformer_testdata("operators"), "src/operators.ts", |s| {
		let want = r"local hasKey = obj[key] ~= nil
flags = bit32.band(flags, mask)
local notNum = not (num ~= 0 and num == num and num)
local _condition = maybeBool
if _condition == nil then
	_condition = true
end
local coalesced = _condition
str ..= tostring(num)
local isFoo = TS.instanceof(inst, Foo)
local fresh = false
local _condition_1 = fresh
if _condition_1 == nil then
	_condition_1 = true
end
local coalesced2 = _condition_1
local cond
local _condition_2 = cond
if _condition_2 == nil then
	_condition_2 = false
end
local coalesced3 = _condition_2
local inverted = bit32.bnot(num)
num ^= mask
num = bit32.arshift(num, mask)
";
		assert_eq!(render_file_statements(s), want);
		// instanceof is the only construct above that touches the runtime lib.
		assert!(
			s.uses_runtime_lib,
			"UsesRuntimeLib = false, want true (instanceof emits TS.instanceof)"
		);
	});
}
