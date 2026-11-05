//! Port of `internal/luau/kind_test.go`.

use crate::SyntaxKind;

#[test]
fn kind_ordering() {
	// Category ranges mirror upstream enums.ts, with rotor's extensions
	// appended after each upstream category's last kind.
	assert_eq!(SyntaxKind::FIRST_INDEXABLE_EXPRESSION, SyntaxKind::Identifier);
	assert_eq!(
		SyntaxKind::LAST_INDEXABLE_EXPRESSION,
		SyntaxKind::ParenthesizedExpression
	);
	// LAST_EXPRESSION is CastExpression, appended after upstream's last
	// expression kind (MixedTable).
	assert_eq!(SyntaxKind::FIRST_EXPRESSION, SyntaxKind::Identifier);
	assert_eq!(SyntaxKind::LAST_EXPRESSION, SyntaxKind::CastExpression);
	// LAST_STATEMENT is TypeFunctionDeclaration; the type statements follow
	// upstream's last statement kind (Comment).
	assert_eq!(SyntaxKind::FIRST_STATEMENT, SyntaxKind::Assignment);
	assert_eq!(SyntaxKind::LAST_STATEMENT, SyntaxKind::TypeFunctionDeclaration);
	assert_eq!(SyntaxKind::FIRST_FIELD, SyntaxKind::MapField);
	assert_eq!(SyntaxKind::LAST_FIELD, SyntaxKind::InterpolatedStringPart);
	assert_eq!(SyntaxKind::FIRST_TYPE, SyntaxKind::TypeReference);
	assert_eq!(SyntaxKind::LAST_TYPE, SyntaxKind::TypeGenericPack);
	assert_eq!(SyntaxKind::FIRST_TYPE_FIELD, SyntaxKind::TypeTableProp);
	assert_eq!(SyntaxKind::LAST_TYPE_FIELD, SyntaxKind::TypeTableIndexer);
	assert!(
		SyntaxKind::LAST_FIELD < SyntaxKind::FIRST_TYPE && SyntaxKind::LAST_TYPE < SyntaxKind::FIRST_TYPE_FIELD,
		"type ranges must follow the field range"
	);
}

#[test]
fn kind_name() {
	let cases = [
		(SyntaxKind::Identifier, "Identifier"),
		(SyntaxKind::ParenthesizedExpression, "ParenthesizedExpression"),
		(SyntaxKind::Set, "Set"),
		(SyntaxKind::CastExpression, "CastExpression"),
		(SyntaxKind::Assignment, "Assignment"),
		(SyntaxKind::Comment, "Comment"),
		(SyntaxKind::TypeAlias, "TypeAlias"),
		(SyntaxKind::TypeFunctionDeclaration, "TypeFunctionDeclaration"),
		(SyntaxKind::MapField, "MapField"),
		(SyntaxKind::InterpolatedString, "InterpolatedString"),
		(SyntaxKind::NumericForStatement, "NumericForStatement"),
		(SyntaxKind::TypeReference, "TypeReference"),
		(SyntaxKind::TypeTableIndexer, "TypeTableIndexer"),
	];
	for (kind, want) in cases {
		assert_eq!(kind.to_string(), want, "kind {} name", kind as u8);
	}
	assert_eq!(SyntaxKind::ALL.len(), SyntaxKind::LAST_TYPE_FIELD as usize + 1);
	for (index, kind) in SyntaxKind::ALL.iter().enumerate() {
		assert_eq!(*kind as usize, index, "ALL is in declaration order");
		assert!(!kind.name().is_empty(), "kind {index} has no name");
	}
}
