//! `diagnostic_string_code_test.go`. tsrs diagnostics carry no string codes
//! (`ast.NewDiagnosticWithStringCode`), so only the numeric-code case ports.

use rotor_tsrs::ast::new_diagnostic;
use rotor_tsrs::diagnostics::IDENTIFIER_EXPECTED;

use crate::compile::info_from_ts_diag;
use crate::source_trace::DiagnosticTraces;

#[test]
#[ignore = "tsrs diagnostics have no string codes (Go's NewDiagnosticWithStringCode, used by plugin diagnostics)"]
fn info_from_ts_diag_prefers_concrete_string_code_and_message() {}

#[test]
fn info_from_ts_diag_preserves_generated_type_script_path() {
	let info = rotor_tsrs::with_large_stack(|| {
		let diagnostic = new_diagnostic(None, None, &IDENTIFIER_EXPECTED, &[]);
		info_from_ts_diag(diagnostic, &DiagnosticTraces::default())
	});
	assert!(
		info.code == "TS1003" && info.message == "Identifier expected." && !info.warning,
		"DiagnosticInfo = {info:?}"
	);
}

#[test]
#[ignore = "tsrs diagnostics have no string codes (Go's NewDiagnosticWithStringCode, used by plugin diagnostics)"]
fn ts_diagnostic_infos_preserves_multiple_ordered_string_code_diagnostics() {}
