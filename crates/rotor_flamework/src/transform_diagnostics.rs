//! `transform_diagnostics.go`: Flamework's diagnostics.
//!
//! Go builds them as `*ast.Diagnostic`, most with rotor's string-code patch
//! (`ast.NewDiagnosticWithStringCode`), which tsrs does not have. [`Diagnostic`] carries the same
//! fields: a string code with the raw message, or a numeric code with the localized message.

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::diagnostics::Category;

/// One Flamework diagnostic, Go's `*ast.Diagnostic`.
#[derive(Clone, Debug)]
pub struct Diagnostic {
	/// The file, if any.
	pub file: Option<P<SourceFile>>,
	/// The start offset; -1 when undefined.
	pub pos: i32,
	/// The end offset; -1 when undefined.
	pub end: i32,
	/// The category.
	pub category: Category,
	/// The numeric code; 0 for a string-coded diagnostic.
	pub code: i32,
	/// The string code, if any (`StringCode`).
	pub string_code: Option<String>,
	/// The message: raw for a string code, localized otherwise.
	pub message: String,
	/// The related information.
	pub related_information: Vec<Self>,
}

impl Diagnostic {
	/// `ast.NewDiagnosticWithStringCode`
	#[must_use]
	pub fn with_string_code(
		file: Option<P<SourceFile>>,
		pos: i32,
		end: i32,
		code: &str,
		category: Category,
		message: impl Into<String>,
	) -> Self {
		Self {
			file,
			pos,
			end,
			category,
			code: 0,
			string_code: Some(code.to_owned()),
			message: message.into(),
			related_information: Vec::new(),
		}
	}

	/// `Pos`
	#[must_use]
	pub const fn pos(&self) -> i32 {
		self.pos
	}

	/// `End`
	#[must_use]
	pub const fn end(&self) -> i32 {
		self.end
	}

	/// `Code`
	#[must_use]
	pub const fn code(&self) -> i32 {
		self.code
	}

	/// `String`: the message.
	#[must_use]
	pub fn string(&self) -> &str {
		&self.message
	}

	/// `AddRelatedInfo`
	pub fn add_related_info(&mut self, related: Self) {
		self.related_information.push(related);
	}
}

/// `diagnosticFileName`
#[must_use]
pub fn diagnostic_file_name(diagnostic: &Diagnostic) -> String {
	diagnostic
		.file
		.map_or_else(String::new, |file| file.file_name().to_owned())
}

/// A node's start (its first token, as `scanner.GetTokenPosOfNode` finds it) and end.
pub(crate) fn node_token_range(node: rotor_tsrs::P<rotor_tsrs::ast::Node>) -> (Option<P<SourceFile>>, i32, i32) {
	let file = rotor_tsrs::ast::get_source_file_of_node(node);
	let start = file.map_or_else(
		|| text_pos(node.pos()),
		|file| i32::try_from(rotor_tsrs::scanner::get_token_pos_of_node(node, file, false)).unwrap_or(i32::MAX),
	);
	(file, start, text_pos(node.end()))
}

/// A `TextPos` as Go's int: -1 for a synthesized position.
pub(crate) fn text_pos(pos: rotor_tsrs::tscore::TextPos) -> i32 {
	pos.source_offset()
		.map_or(-1, |offset| i32::try_from(offset).unwrap_or(i32::MAX))
}

/// `flameworkNodeDiagnostic`
pub(crate) fn flamework_node_diagnostic(
	node: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	category: Category,
	message: &str,
) -> Diagnostic {
	let (file, start, end) = node_token_range(node);
	Diagnostic::with_string_code(
		file,
		start,
		end,
		crate::project::FLAMEWORK_DIAGNOSTIC_CODE,
		category,
		message,
	)
}

/// `captureFlameworkDiagnostic`: records a macro or guard error as a diagnostic.
pub(crate) fn capture_flamework_diagnostic(
	state: &mut crate::transform_state::TransformState<'_>,
	error: &crate::Error,
) -> bool {
	match error.detail() {
		Some(crate::ErrorDetail::Macro(macro_error)) => {
			let Some(node) = macro_error.node else {
				return false;
			};
			let mut diagnostic = flamework_node_diagnostic(node, Category::Error, &macro_error.message);
			for related in &macro_error.related_information {
				if let Some(node) = related.node {
					diagnostic.add_related_info(flamework_node_diagnostic(node, Category::Message, &related.message));
				}
			}
			state.add_diagnostic(diagnostic);
			true
		}
		Some(crate::ErrorDetail::Guard(guard_error)) => {
			let file = state.program.get_source_file(&guard_error.file_name);
			let mut diagnostic = Diagnostic::with_string_code(
				file,
				guard_error.start,
				guard_error.end,
				crate::project::FLAMEWORK_DIAGNOSTIC_CODE,
				Category::Error,
				guard_error.message(),
			);
			for related in &guard_error.related_information {
				let related_file = state.program.get_source_file(&related.file_name);
				diagnostic.add_related_info(Diagnostic::with_string_code(
					related_file,
					related.start,
					related.end,
					crate::project::FLAMEWORK_DIAGNOSTIC_CODE,
					Category::Error,
					format!("Type was defined here: {}", related.type_name),
				));
			}
			state.add_diagnostic(diagnostic);
			true
		}
		_ => false,
	}
}

/// `ast.NewDiagnostic` for a TypeScript message.
pub(crate) fn new_message_diagnostic(
	file: Option<P<SourceFile>>,
	pos: i32,
	end: i32,
	message: &rotor_tsrs::diagnostics::Message,
	args: &[&str],
) -> Diagnostic {
	let displays: Vec<&dyn std::fmt::Display> = args.iter().map(|arg| arg as &dyn std::fmt::Display).collect();
	Diagnostic {
		file,
		pos,
		end,
		category: message.category(),
		code: message.code(),
		string_code: None,
		message: message.localize(&displays),
		related_information: Vec::new(),
	}
}
