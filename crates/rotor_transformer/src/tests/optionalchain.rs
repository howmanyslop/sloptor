//! `optionalchain_test.go`. The chain shape and macro interplay tests need the macros (slice iv).

use super::{render_file_statements, transformer_testdata, with_state};

#[test]
fn optional_chain_access() {
	with_state(&transformer_testdata("calls"), "src/optional.ts", |s| {
		let want = r"local _n = holder.value
if _n ~= nil then
	_n = _n.num
end
local n = _n
print(n)
";
		assert_eq!(render_file_statements(s), want);
	});
}
