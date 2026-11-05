//! `guard_diagnostics.go`: the error a guard that cannot be generated raises.

use rotor_support::gostr::quote;

use crate::{Error, ErrorDetail, ErrorKind};

/// `GuardGenerationError`
#[derive(Clone, Debug, Default)]
pub struct GuardGenerationError {
	/// The type's text.
	pub type_name: String,
	/// Why the guard cannot be generated.
	pub reason: String,
	/// The source's file.
	pub file_name: String,
	/// The source's start.
	pub start: i32,
	/// The source's end.
	pub end: i32,
	/// `file:type` for each type on the way to the failure.
	pub path: Vec<String>,
	/// Where each type on the way was defined.
	pub related_information: Vec<GuardRelatedInformation>,
}

/// `GuardRelatedInformation`
#[derive(Clone, Debug, Default)]
pub struct GuardRelatedInformation {
	/// The type's text.
	pub type_name: String,
	/// The declaring file.
	pub file_name: String,
	/// The declaration's start.
	pub start: i32,
	/// The declaration's end.
	pub end: i32,
}

impl GuardGenerationError {
	/// `Error`
	#[must_use]
	pub fn message(&self) -> String {
		let reason = self.reason.as_str();
		if reason == "template literal types are unsupported" {
			return format!(
				"Flamework encountered a template literal which is unsupported: {}",
				self.type_name
			);
		}
		if reason == "could not find constraint of type parameter" {
			return reason.to_owned();
		}
		if reason == "unknown type has no symbol" {
			return format!("An unknown type was encountered with no symbol: {}", self.type_name);
		}
		if reason == "intersection between nominal types is forbidden" {
			return "Intersection between nominal types is forbidden.".to_owned();
		}
		if reason.starts_with("class ") && reason.ends_with(" is unsupported") {
			return format!(
				"Class {} was encountered. Flamework does not support generating guards for classes.",
				quote(&self.type_name)
			);
		}
		if self.path.is_empty() {
			return format!("generate guard for {}: {reason}", self.type_name);
		}
		format!(
			"generate guard for {}: {reason} (type path: [{}])",
			self.type_name,
			self.path.join(" ")
		)
	}

	/// The error, matched by `ErrUnsupportedGuardType`.
	#[must_use]
	pub fn into_error(self) -> Error {
		Error::with_detail(self.message(), ErrorDetail::Guard(self)).also(ErrorKind::UnsupportedGuardType)
	}
}
