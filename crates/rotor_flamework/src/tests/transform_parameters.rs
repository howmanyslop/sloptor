//! `transform_parameters_test.go`

use super::{collect_call_expressions, with_macro_state};
use crate::ErrorKind;
use crate::transform_parameters::validate_parameter_const_intrinsic;

/// The single call's const-intrinsic validation: `Ok`, or the error's sentinel match and message.
fn validate(source: &str) -> Result<(), (bool, String)> {
	with_macro_state(
		rotor_config::config::FlameworkConfig::default(),
		source,
		|state, file| {
			let calls = collect_call_expressions(file);
			assert_eq!(calls.len(), 1, "calls");
			let signature = state.ck().get_resolved_signature(calls[0]);
			validate_parameter_const_intrinsic(calls[0], signature, signature.parameters())
				.map_err(|error| (error.is(ErrorKind::MacroDiagnostic), error.message().to_owned()))
		},
	)
}

#[test]
fn validate_parameter_const_intrinsic_rejects_spread_at_literal_depth_one() {
	let error = validate(
		"\ndeclare function inspect(value: object): void;\nconst other = { count: 1 };\ninspect({ ...other });",
	)
	.expect_err("validateParameterConstIntrinsic() succeeded, want spread rejection");

	assert_eq!(
		error,
		(
			true,
			"Flamework does not support spread expressions in this location.".to_owned()
		)
	);
}

#[test]
fn validate_parameter_const_intrinsic_accepts_literal_object_at_depth_one() {
	let result = validate("\ndeclare function inspect(value: object): void;\ninspect({ nested: dynamicValue });");

	assert_eq!(result, Ok(()));
}
