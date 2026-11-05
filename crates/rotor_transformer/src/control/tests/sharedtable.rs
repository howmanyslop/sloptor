//! The `TransformStatementList` half of `sharedtable_test.go`: a `SharedTable` iterates like a
//! `Map`.

use rotor_tsrs::types::is_shared_table_type;

use crate::statementlist::transform_statement_list;
use crate::tests::{collect_identifiers, render_list, transformer_testdata, with_state};

#[test]
fn shared_table() {
	let got = with_state(&transformer_testdata("forof"), "src/sharedtable.ts", |s| {
		assert!(
			s.ambient_symbol("SharedTable").is_some(),
			"SharedTable symbol was not registered"
		);
		let identifiers = collect_identifiers(s.source_file.as_node(), "t");
		assert!(!identifiers.is_empty(), "no identifiers named 't' found");
		let t = s.get_type(identifiers[0]);
		assert!(
			is_shared_table_type(s, t),
			"SharedTable type was not detected by its checker symbol"
		);
		let file = s.source_file;
		let statements = transform_statement_list(s, Some(file.as_node()), file.statements().nodes(), None);
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		render_list(s, statements)
	});
	let want = "local function iterateTable(t)
	local sum = 0
	for k, v in t do
		sum += v
	end
	return sum
end
";
	assert_eq!(got, want);
}
