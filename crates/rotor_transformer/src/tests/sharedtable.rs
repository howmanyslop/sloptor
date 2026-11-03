//! The type half of `sharedtable_test.go`; its `TransformStatementList` half compiles a function
//! declaration and a for-of loop, slice (ii)'s.

use rotor_tsrs::types::is_shared_table_type;

use super::{collect_identifiers, transformer_testdata, with_state};

#[test]
fn shared_table_type() {
	with_state(&transformer_testdata("forof"), "src/sharedtable.ts", |s| {
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
	});
}
