# rotor_tsrs API inventory

Every typescript-go API that non-mirrored Go code calls, mapped to tsrs and to its home in `rotor_tsrs`.

- **Scope:** the non-test files of `internal/transformer`, `internal/compile`, `internal/flamework`, `internal/jecs` and
  `internal/lsp`. Each row is one function, method, field, constant, type or variable of a `rotor/tsgo/...` package that
  this code names, found by type-checking the packages with `go/packages`, so method calls resolve to their receiver type.
- **Sites** counts the references and lists the packages that make them.
- **tsrs** is the counterpart in the vendored crates. Names were checked against `third_party/tsrs` sources;
  "(0001 patch)", "(0002 patch)" and "(0004 patch)" mark items `tools/tsrs-mirror/patches` makes public
  or adds.
- **rotor_tsrs** is where later crates reach it. Re-exported modules keep the tsrs path under `rotor_tsrs`
  (`rotor_tsrs::ast::Node::name` is `tsrs_ast::Node::name`). Checker queries go through
  `rotor_tsrs::checker::TypeChecker`, which names each one after its Go method and derefs to the tsrs `Checker`.

## Totals

- 1441 APIs over 8894 reference sites: **1323 covered**, **118 missing**.
- 113 of the missing are LSP protocol, JSON-RPC, language-service and watcher APIs. tsrs vendors no such crates, and
  they belong to Stage 7 (`rotor_lsp`, `rotor_cli`).
- The other 5 are rotor-only tsgo patches or APIs tsrs 0.20.1 does not have; each row gives the reason.

| Package | Count |
|---|---|
| `ast` | 879 (2 missing) |
| `bundled` | 2 |
| `checker` | 180 |
| `collections` | 9 |
| `compiler` | 54 (2 missing) |
| `core` | 68 |
| `diagnostics` | 9 |
| `jsnum` | 1 |
| `json` | 4 (1 missing) |
| `jsonrpc` | 22 (22 missing) |
| `ls/lsconv` | 5 (5 missing) |
| `ls/lsutil` | 1 (1 missing) |
| `lsp/lsproto` | 85 (85 missing) |
| `module` | 14 |
| `nodebuilder` | 2 |
| `outputpaths` | 5 |
| `parser` | 2 |
| `printer` | 20 |
| `scanner` | 9 |
| `sourcemap` | 12 |
| `stringutil` | 4 |
| `symlinks` | 2 |
| `tsoptions` | 9 |
| `tspath` | 28 |
| `vfs` | 6 |
| `vfs/cachedvfs` | 4 |
| `vfs/osvfs` | 1 |
| `vfs/wrapvfs` | 4 |

## Inventory

| Package | Kind | Go API | Sites | tsrs | rotor_tsrs |
|---|---|---|---|---|---|
| `ast` | const | `CheckFlagsReadonly` | 1 (transformer) | `CheckFlags::Readonly` | `rotor_tsrs::ast::CheckFlags::Readonly` |
| `ast` | const | `FunctionFlagsGenerator` | 1 (transformer) | `FunctionFlags::Generator` | `rotor_tsrs::ast::FunctionFlags::Generator` |
| `ast` | const | `InternalSymbolNameClass` | 1 (transformer) | `INTERNAL_SYMBOL_NAME_CLASS` | `rotor_tsrs::ast::INTERNAL_SYMBOL_NAME_CLASS` |
| `ast` | const | `InternalSymbolNameDefault` | 1 (transformer) | `INTERNAL_SYMBOL_NAME_DEFAULT` | `rotor_tsrs::ast::INTERNAL_SYMBOL_NAME_DEFAULT` |
| `ast` | const | `InternalSymbolNameMissing` | 2 (transformer) | `INTERNAL_SYMBOL_NAME_MISSING` | `rotor_tsrs::ast::INTERNAL_SYMBOL_NAME_MISSING` |
| `ast` | const | `InternalSymbolNamePrefix` | 2 (transformer) | `INTERNAL_SYMBOL_NAME_PREFIX` | `rotor_tsrs::ast::INTERNAL_SYMBOL_NAME_PREFIX` |
| `ast` | const | `KindAccessorKeyword` | 1 (transformer) | `Kind::AccessorKeyword` | `rotor_tsrs::ast::Kind::AccessorKeyword` |
| `ast` | const | `KindAmpersandAmpersandEqualsToken` | 2 (flamework,transformer) | `Kind::AmpersandAmpersandEqualsToken` | `rotor_tsrs::ast::Kind::AmpersandAmpersandEqualsToken` |
| `ast` | const | `KindAmpersandAmpersandToken` | 5 (flamework,jecs,transformer) | `Kind::AmpersandAmpersandToken` | `rotor_tsrs::ast::Kind::AmpersandAmpersandToken` |
| `ast` | const | `KindAmpersandEqualsToken` | 2 (flamework,transformer) | `Kind::AmpersandEqualsToken` | `rotor_tsrs::ast::Kind::AmpersandEqualsToken` |
| `ast` | const | `KindAmpersandToken` | 3 (flamework,transformer) | `Kind::AmpersandToken` | `rotor_tsrs::ast::Kind::AmpersandToken` |
| `ast` | const | `KindAnyKeyword` | 2 (transformer) | `Kind::AnyKeyword` | `rotor_tsrs::ast::Kind::AnyKeyword` |
| `ast` | const | `KindArrayLiteralExpression` | 2 (flamework,transformer) | `Kind::ArrayLiteralExpression` | `rotor_tsrs::ast::Kind::ArrayLiteralExpression` |
| `ast` | const | `KindArrayType` | 4 (transformer) | `Kind::ArrayType` | `rotor_tsrs::ast::Kind::ArrayType` |
| `ast` | const | `KindArrowFunction` | 2 (transformer) | `Kind::ArrowFunction` | `rotor_tsrs::ast::Kind::ArrowFunction` |
| `ast` | const | `KindAsExpression` | 3 (transformer) | `Kind::AsExpression` | `rotor_tsrs::ast::Kind::AsExpression` |
| `ast` | const | `KindAsteriskAsteriskEqualsToken` | 2 (flamework,transformer) | `Kind::AsteriskAsteriskEqualsToken` | `rotor_tsrs::ast::Kind::AsteriskAsteriskEqualsToken` |
| `ast` | const | `KindAsteriskAsteriskToken` | 3 (flamework,transformer) | `Kind::AsteriskAsteriskToken` | `rotor_tsrs::ast::Kind::AsteriskAsteriskToken` |
| `ast` | const | `KindAsteriskEqualsToken` | 2 (flamework,transformer) | `Kind::AsteriskEqualsToken` | `rotor_tsrs::ast::Kind::AsteriskEqualsToken` |
| `ast` | const | `KindAsteriskToken` | 4 (flamework,jecs,transformer) | `Kind::AsteriskToken` | `rotor_tsrs::ast::Kind::AsteriskToken` |
| `ast` | const | `KindAwaitExpression` | 1 (transformer) | `Kind::AwaitExpression` | `rotor_tsrs::ast::Kind::AwaitExpression` |
| `ast` | const | `KindBarBarEqualsToken` | 1 (flamework) | `Kind::BarBarEqualsToken` | `rotor_tsrs::ast::Kind::BarBarEqualsToken` |
| `ast` | const | `KindBarBarToken` | 4 (flamework,jecs,transformer) | `Kind::BarBarToken` | `rotor_tsrs::ast::Kind::BarBarToken` |
| `ast` | const | `KindBarEqualsToken` | 2 (flamework,transformer) | `Kind::BarEqualsToken` | `rotor_tsrs::ast::Kind::BarEqualsToken` |
| `ast` | const | `KindBarToken` | 3 (flamework,transformer) | `Kind::BarToken` | `rotor_tsrs::ast::Kind::BarToken` |
| `ast` | const | `KindBigIntKeyword` | 1 (transformer) | `Kind::BigIntKeyword` | `rotor_tsrs::ast::Kind::BigIntKeyword` |
| `ast` | const | `KindBigIntLiteral` | 3 (transformer) | `Kind::BigIntLiteral` | `rotor_tsrs::ast::Kind::BigIntLiteral` |
| `ast` | const | `KindBinaryExpression` | 3 (flamework,transformer) | `Kind::BinaryExpression` | `rotor_tsrs::ast::Kind::BinaryExpression` |
| `ast` | const | `KindBlock` | 4 (transformer) | `Kind::Block` | `rotor_tsrs::ast::Kind::Block` |
| `ast` | const | `KindBooleanKeyword` | 1 (transformer) | `Kind::BooleanKeyword` | `rotor_tsrs::ast::Kind::BooleanKeyword` |
| `ast` | const | `KindBreakStatement` | 3 (transformer) | `Kind::BreakStatement` | `rotor_tsrs::ast::Kind::BreakStatement` |
| `ast` | const | `KindCallExpression` | 4 (compile,flamework,transformer) | `Kind::CallExpression` | `rotor_tsrs::ast::Kind::CallExpression` |
| `ast` | const | `KindCaretEqualsToken` | 2 (flamework,transformer) | `Kind::CaretEqualsToken` | `rotor_tsrs::ast::Kind::CaretEqualsToken` |
| `ast` | const | `KindCaretToken` | 3 (flamework,transformer) | `Kind::CaretToken` | `rotor_tsrs::ast::Kind::CaretToken` |
| `ast` | const | `KindCaseClause` | 2 (transformer) | `Kind::CaseClause` | `rotor_tsrs::ast::Kind::CaseClause` |
| `ast` | const | `KindClassDeclaration` | 1 (transformer) | `Kind::ClassDeclaration` | `rotor_tsrs::ast::Kind::ClassDeclaration` |
| `ast` | const | `KindClassExpression` | 1 (transformer) | `Kind::ClassExpression` | `rotor_tsrs::ast::Kind::ClassExpression` |
| `ast` | const | `KindColonToken` | 1 (jecs) | `Kind::ColonToken` | `rotor_tsrs::ast::Kind::ColonToken` |
| `ast` | const | `KindCommaToken` | 1 (transformer) | `Kind::CommaToken` | `rotor_tsrs::ast::Kind::CommaToken` |
| `ast` | const | `KindConditionalExpression` | 1 (transformer) | `Kind::ConditionalExpression` | `rotor_tsrs::ast::Kind::ConditionalExpression` |
| `ast` | const | `KindConditionalType` | 4 (transformer) | `Kind::ConditionalType` | `rotor_tsrs::ast::Kind::ConditionalType` |
| `ast` | const | `KindConstructor` | 1 (transformer) | `Kind::Constructor` | `rotor_tsrs::ast::Kind::Constructor` |
| `ast` | const | `KindContinueStatement` | 3 (transformer) | `Kind::ContinueStatement` | `rotor_tsrs::ast::Kind::ContinueStatement` |
| `ast` | const | `KindDebuggerStatement` | 1 (transformer) | `Kind::DebuggerStatement` | `rotor_tsrs::ast::Kind::DebuggerStatement` |
| `ast` | const | `KindDeclareKeyword` | 1 (transformer) | `Kind::DeclareKeyword` | `rotor_tsrs::ast::Kind::DeclareKeyword` |
| `ast` | const | `KindDecorator` | 1 (transformer) | `Kind::Decorator` | `rotor_tsrs::ast::Kind::Decorator` |
| `ast` | const | `KindDefaultClause` | 2 (transformer) | `Kind::DefaultClause` | `rotor_tsrs::ast::Kind::DefaultClause` |
| `ast` | const | `KindDeleteExpression` | 2 (flamework,transformer) | `Kind::DeleteExpression` | `rotor_tsrs::ast::Kind::DeleteExpression` |
| `ast` | const | `KindDoStatement` | 1 (transformer) | `Kind::DoStatement` | `rotor_tsrs::ast::Kind::DoStatement` |
| `ast` | const | `KindElementAccessExpression` | 3 (compile,flamework,transformer) | `Kind::ElementAccessExpression` | `rotor_tsrs::ast::Kind::ElementAccessExpression` |
| `ast` | const | `KindEmptyStatement` | 1 (transformer) | `Kind::EmptyStatement` | `rotor_tsrs::ast::Kind::EmptyStatement` |
| `ast` | const | `KindEnumDeclaration` | 1 (transformer) | `Kind::EnumDeclaration` | `rotor_tsrs::ast::Kind::EnumDeclaration` |
| `ast` | const | `KindEqualsEqualsEqualsToken` | 3 (jecs,transformer) | `Kind::EqualsEqualsEqualsToken` | `rotor_tsrs::ast::Kind::EqualsEqualsEqualsToken` |
| `ast` | const | `KindEqualsEqualsToken` | 1 (transformer) | `Kind::EqualsEqualsToken` | `rotor_tsrs::ast::Kind::EqualsEqualsToken` |
| `ast` | const | `KindEqualsToken` | 5 (flamework,transformer) | `Kind::EqualsToken` | `rotor_tsrs::ast::Kind::EqualsToken` |
| `ast` | const | `KindExclamationEqualsEqualsToken` | 3 (jecs,transformer) | `Kind::ExclamationEqualsEqualsToken` | `rotor_tsrs::ast::Kind::ExclamationEqualsEqualsToken` |
| `ast` | const | `KindExclamationEqualsToken` | 1 (transformer) | `Kind::ExclamationEqualsToken` | `rotor_tsrs::ast::Kind::ExclamationEqualsToken` |
| `ast` | const | `KindExclamationToken` | 3 (jecs,transformer) | `Kind::ExclamationToken` | `rotor_tsrs::ast::Kind::ExclamationToken` |
| `ast` | const | `KindExportAssignment` | 1 (transformer) | `Kind::ExportAssignment` | `rotor_tsrs::ast::Kind::ExportAssignment` |
| `ast` | const | `KindExportDeclaration` | 2 (compile,transformer) | `Kind::ExportDeclaration` | `rotor_tsrs::ast::Kind::ExportDeclaration` |
| `ast` | const | `KindExpressionStatement` | 1 (transformer) | `Kind::ExpressionStatement` | `rotor_tsrs::ast::Kind::ExpressionStatement` |
| `ast` | const | `KindExpressionWithTypeArguments` | 1 (transformer) | `Kind::ExpressionWithTypeArguments` | `rotor_tsrs::ast::Kind::ExpressionWithTypeArguments` |
| `ast` | const | `KindExtendsKeyword` | 1 (transformer) | `Kind::ExtendsKeyword` | `rotor_tsrs::ast::Kind::ExtendsKeyword` |
| `ast` | const | `KindExternalModuleReference` | 1 (compile) | `Kind::ExternalModuleReference` | `rotor_tsrs::ast::Kind::ExternalModuleReference` |
| `ast` | const | `KindFalseKeyword` | 7 (flamework,transformer) | `Kind::FalseKeyword` | `rotor_tsrs::ast::Kind::FalseKeyword` |
| `ast` | const | `KindForInStatement` | 1 (transformer) | `Kind::ForInStatement` | `rotor_tsrs::ast::Kind::ForInStatement` |
| `ast` | const | `KindForOfStatement` | 3 (jecs,transformer) | `Kind::ForOfStatement` | `rotor_tsrs::ast::Kind::ForOfStatement` |
| `ast` | const | `KindForStatement` | 1 (transformer) | `Kind::ForStatement` | `rotor_tsrs::ast::Kind::ForStatement` |
| `ast` | const | `KindFunctionDeclaration` | 3 (transformer) | `Kind::FunctionDeclaration` | `rotor_tsrs::ast::Kind::FunctionDeclaration` |
| `ast` | const | `KindFunctionExpression` | 3 (transformer) | `Kind::FunctionExpression` | `rotor_tsrs::ast::Kind::FunctionExpression` |
| `ast` | const | `KindFunctionType` | 2 (transformer) | `Kind::FunctionType` | `rotor_tsrs::ast::Kind::FunctionType` |
| `ast` | const | `KindGreaterThanEqualsToken` | 3 (transformer) | `Kind::GreaterThanEqualsToken` | `rotor_tsrs::ast::Kind::GreaterThanEqualsToken` |
| `ast` | const | `KindGreaterThanGreaterThanEqualsToken` | 2 (flamework,transformer) | `Kind::GreaterThanGreaterThanEqualsToken` | `rotor_tsrs::ast::Kind::GreaterThanGreaterThanEqualsToken` |
| `ast` | const | `KindGreaterThanGreaterThanGreaterThanEqualsToken` | 2 (flamework,transformer) | `Kind::GreaterThanGreaterThanGreaterThanEqualsToken` | `rotor_tsrs::ast::Kind::GreaterThanGreaterThanGreaterThanEqualsToken` |
| `ast` | const | `KindGreaterThanGreaterThanGreaterThanToken` | 2 (flamework,transformer) | `Kind::GreaterThanGreaterThanGreaterThanToken` | `rotor_tsrs::ast::Kind::GreaterThanGreaterThanGreaterThanToken` |
| `ast` | const | `KindGreaterThanGreaterThanToken` | 2 (flamework,transformer) | `Kind::GreaterThanGreaterThanToken` | `rotor_tsrs::ast::Kind::GreaterThanGreaterThanToken` |
| `ast` | const | `KindGreaterThanToken` | 4 (transformer) | `Kind::GreaterThanToken` | `rotor_tsrs::ast::Kind::GreaterThanToken` |
| `ast` | const | `KindIdentifier` | 4 (flamework,transformer) | `Kind::Identifier` | `rotor_tsrs::ast::Kind::Identifier` |
| `ast` | const | `KindIfStatement` | 2 (transformer) | `Kind::IfStatement` | `rotor_tsrs::ast::Kind::IfStatement` |
| `ast` | const | `KindImplementsKeyword` | 1 (flamework) | `Kind::ImplementsKeyword` | `rotor_tsrs::ast::Kind::ImplementsKeyword` |
| `ast` | const | `KindImportDeclaration` | 3 (compile,transformer) | `Kind::ImportDeclaration` | `rotor_tsrs::ast::Kind::ImportDeclaration` |
| `ast` | const | `KindImportEqualsDeclaration` | 2 (transformer) | `Kind::ImportEqualsDeclaration` | `rotor_tsrs::ast::Kind::ImportEqualsDeclaration` |
| `ast` | const | `KindImportKeyword` | 3 (compile,flamework,transformer) | `Kind::ImportKeyword` | `rotor_tsrs::ast::Kind::ImportKeyword` |
| `ast` | const | `KindImportType` | 1 (compile) | `Kind::ImportType` | `rotor_tsrs::ast::Kind::ImportType` |
| `ast` | const | `KindInKeyword` | 1 (transformer) | `Kind::InKeyword` | `rotor_tsrs::ast::Kind::InKeyword` |
| `ast` | const | `KindIndexSignature` | 1 (transformer) | `Kind::IndexSignature` | `rotor_tsrs::ast::Kind::IndexSignature` |
| `ast` | const | `KindIndexedAccessType` | 1 (transformer) | `Kind::IndexedAccessType` | `rotor_tsrs::ast::Kind::IndexedAccessType` |
| `ast` | const | `KindInferType` | 5 (transformer) | `Kind::InferType` | `rotor_tsrs::ast::Kind::InferType` |
| `ast` | const | `KindInstanceOfKeyword` | 1 (transformer) | `Kind::InstanceOfKeyword` | `rotor_tsrs::ast::Kind::InstanceOfKeyword` |
| `ast` | const | `KindInterfaceDeclaration` | 1 (transformer) | `Kind::InterfaceDeclaration` | `rotor_tsrs::ast::Kind::InterfaceDeclaration` |
| `ast` | const | `KindIntersectionType` | 1 (transformer) | `Kind::IntersectionType` | `rotor_tsrs::ast::Kind::IntersectionType` |
| `ast` | const | `KindJSDocUnknownTag` | 1 (flamework) | `Kind::JSDocUnknownTag` | `rotor_tsrs::ast::Kind::JSDocUnknownTag` |
| `ast` | const | `KindJsxElement` | 1 (transformer) | `Kind::JsxElement` | `rotor_tsrs::ast::Kind::JsxElement` |
| `ast` | const | `KindJsxExpression` | 1 (transformer) | `Kind::JsxExpression` | `rotor_tsrs::ast::Kind::JsxExpression` |
| `ast` | const | `KindJsxFragment` | 1 (transformer) | `Kind::JsxFragment` | `rotor_tsrs::ast::Kind::JsxFragment` |
| `ast` | const | `KindJsxSelfClosingElement` | 1 (transformer) | `Kind::JsxSelfClosingElement` | `rotor_tsrs::ast::Kind::JsxSelfClosingElement` |
| `ast` | const | `KindKeyOfKeyword` | 3 (transformer) | `Kind::KeyOfKeyword` | `rotor_tsrs::ast::Kind::KeyOfKeyword` |
| `ast` | const | `KindLabeledStatement` | 1 (transformer) | `Kind::LabeledStatement` | `rotor_tsrs::ast::Kind::LabeledStatement` |
| `ast` | const | `KindLessThanEqualsToken` | 4 (jecs,transformer) | `Kind::LessThanEqualsToken` | `rotor_tsrs::ast::Kind::LessThanEqualsToken` |
| `ast` | const | `KindLessThanLessThanEqualsToken` | 2 (flamework,transformer) | `Kind::LessThanLessThanEqualsToken` | `rotor_tsrs::ast::Kind::LessThanLessThanEqualsToken` |
| `ast` | const | `KindLessThanLessThanToken` | 2 (flamework,transformer) | `Kind::LessThanLessThanToken` | `rotor_tsrs::ast::Kind::LessThanLessThanToken` |
| `ast` | const | `KindLessThanToken` | 4 (transformer) | `Kind::LessThanToken` | `rotor_tsrs::ast::Kind::LessThanToken` |
| `ast` | const | `KindLiteralType` | 3 (compile,transformer) | `Kind::LiteralType` | `rotor_tsrs::ast::Kind::LiteralType` |
| `ast` | const | `KindMappedType` | 4 (transformer) | `Kind::MappedType` | `rotor_tsrs::ast::Kind::MappedType` |
| `ast` | const | `KindMethodDeclaration` | 3 (transformer) | `Kind::MethodDeclaration` | `rotor_tsrs::ast::Kind::MethodDeclaration` |
| `ast` | const | `KindMinusEqualsToken` | 3 (flamework,transformer) | `Kind::MinusEqualsToken` | `rotor_tsrs::ast::Kind::MinusEqualsToken` |
| `ast` | const | `KindMinusMinusToken` | 8 (flamework,transformer) | `Kind::MinusMinusToken` | `rotor_tsrs::ast::Kind::MinusMinusToken` |
| `ast` | const | `KindMinusToken` | 10 (flamework,jecs,transformer) | `Kind::MinusToken` | `rotor_tsrs::ast::Kind::MinusToken` |
| `ast` | const | `KindModuleBlock` | 1 (transformer) | `Kind::ModuleBlock` | `rotor_tsrs::ast::Kind::ModuleBlock` |
| `ast` | const | `KindModuleDeclaration` | 1 (transformer) | `Kind::ModuleDeclaration` | `rotor_tsrs::ast::Kind::ModuleDeclaration` |
| `ast` | const | `KindNamedTupleMember` | 1 (transformer) | `Kind::NamedTupleMember` | `rotor_tsrs::ast::Kind::NamedTupleMember` |
| `ast` | const | `KindNamespaceKeyword` | 1 (transformer) | `Kind::NamespaceKeyword` | `rotor_tsrs::ast::Kind::NamespaceKeyword` |
| `ast` | const | `KindNeverKeyword` | 2 (flamework,transformer) | `Kind::NeverKeyword` | `rotor_tsrs::ast::Kind::NeverKeyword` |
| `ast` | const | `KindNewExpression` | 2 (flamework,transformer) | `Kind::NewExpression` | `rotor_tsrs::ast::Kind::NewExpression` |
| `ast` | const | `KindNoSubstitutionTemplateLiteral` | 4 (transformer) | `Kind::NoSubstitutionTemplateLiteral` | `rotor_tsrs::ast::Kind::NoSubstitutionTemplateLiteral` |
| `ast` | const | `KindNonNullExpression` | 3 (transformer) | `Kind::NonNullExpression` | `rotor_tsrs::ast::Kind::NonNullExpression` |
| `ast` | const | `KindNullKeyword` | 7 (flamework,transformer) | `Kind::NullKeyword` | `rotor_tsrs::ast::Kind::NullKeyword` |
| `ast` | const | `KindNumberKeyword` | 1 (transformer) | `Kind::NumberKeyword` | `rotor_tsrs::ast::Kind::NumberKeyword` |
| `ast` | const | `KindNumericLiteral` | 6 (flamework,transformer) | `Kind::NumericLiteral` | `rotor_tsrs::ast::Kind::NumericLiteral` |
| `ast` | const | `KindObjectKeyword` | 1 (transformer) | `Kind::ObjectKeyword` | `rotor_tsrs::ast::Kind::ObjectKeyword` |
| `ast` | const | `KindObjectLiteralExpression` | 2 (flamework,transformer) | `Kind::ObjectLiteralExpression` | `rotor_tsrs::ast::Kind::ObjectLiteralExpression` |
| `ast` | const | `KindOptionalType` | 1 (transformer) | `Kind::OptionalType` | `rotor_tsrs::ast::Kind::OptionalType` |
| `ast` | const | `KindParameter` | 2 (transformer) | `Kind::Parameter` | `rotor_tsrs::ast::Kind::Parameter` |
| `ast` | const | `KindParenthesizedExpression` | 4 (compile,transformer) | `Kind::ParenthesizedExpression` | `rotor_tsrs::ast::Kind::ParenthesizedExpression` |
| `ast` | const | `KindParenthesizedType` | 2 (compile,transformer) | `Kind::ParenthesizedType` | `rotor_tsrs::ast::Kind::ParenthesizedType` |
| `ast` | const | `KindPercentEqualsToken` | 2 (flamework,transformer) | `Kind::PercentEqualsToken` | `rotor_tsrs::ast::Kind::PercentEqualsToken` |
| `ast` | const | `KindPercentToken` | 3 (flamework,jecs,transformer) | `Kind::PercentToken` | `rotor_tsrs::ast::Kind::PercentToken` |
| `ast` | const | `KindPlusEqualsToken` | 4 (flamework,transformer) | `Kind::PlusEqualsToken` | `rotor_tsrs::ast::Kind::PlusEqualsToken` |
| `ast` | const | `KindPlusPlusToken` | 6 (flamework,transformer) | `Kind::PlusPlusToken` | `rotor_tsrs::ast::Kind::PlusPlusToken` |
| `ast` | const | `KindPlusToken` | 8 (flamework,transformer) | `Kind::PlusToken` | `rotor_tsrs::ast::Kind::PlusToken` |
| `ast` | const | `KindPostfixUnaryExpression` | 2 (flamework,transformer) | `Kind::PostfixUnaryExpression` | `rotor_tsrs::ast::Kind::PostfixUnaryExpression` |
| `ast` | const | `KindPrefixUnaryExpression` | 4 (flamework,transformer) | `Kind::PrefixUnaryExpression` | `rotor_tsrs::ast::Kind::PrefixUnaryExpression` |
| `ast` | const | `KindPrivateIdentifier` | 3 (transformer) | `Kind::PrivateIdentifier` | `rotor_tsrs::ast::Kind::PrivateIdentifier` |
| `ast` | const | `KindPropertyAccessExpression` | 4 (compile,flamework,transformer) | `Kind::PropertyAccessExpression` | `rotor_tsrs::ast::Kind::PropertyAccessExpression` |
| `ast` | const | `KindPropertyAssignment` | 1 (transformer) | `Kind::PropertyAssignment` | `rotor_tsrs::ast::Kind::PropertyAssignment` |
| `ast` | const | `KindPropertySignature` | 1 (transformer) | `Kind::PropertySignature` | `rotor_tsrs::ast::Kind::PropertySignature` |
| `ast` | const | `KindQualifiedName` | 1 (transformer) | `Kind::QualifiedName` | `rotor_tsrs::ast::Kind::QualifiedName` |
| `ast` | const | `KindQuestionDotToken` | 1 (jecs) | `Kind::QuestionDotToken` | `rotor_tsrs::ast::Kind::QuestionDotToken` |
| `ast` | const | `KindQuestionQuestionEqualsToken` | 2 (flamework,transformer) | `Kind::QuestionQuestionEqualsToken` | `rotor_tsrs::ast::Kind::QuestionQuestionEqualsToken` |
| `ast` | const | `KindQuestionQuestionToken` | 4 (flamework,transformer) | `Kind::QuestionQuestionToken` | `rotor_tsrs::ast::Kind::QuestionQuestionToken` |
| `ast` | const | `KindQuestionToken` | 2 (jecs,transformer) | `Kind::QuestionToken` | `rotor_tsrs::ast::Kind::QuestionToken` |
| `ast` | const | `KindReadonlyKeyword` | 2 (transformer) | `Kind::ReadonlyKeyword` | `rotor_tsrs::ast::Kind::ReadonlyKeyword` |
| `ast` | const | `KindRegularExpressionLiteral` | 1 (transformer) | `Kind::RegularExpressionLiteral` | `rotor_tsrs::ast::Kind::RegularExpressionLiteral` |
| `ast` | const | `KindRestType` | 1 (transformer) | `Kind::RestType` | `rotor_tsrs::ast::Kind::RestType` |
| `ast` | const | `KindReturnStatement` | 2 (transformer) | `Kind::ReturnStatement` | `rotor_tsrs::ast::Kind::ReturnStatement` |
| `ast` | const | `KindSatisfiesExpression` | 3 (transformer) | `Kind::SatisfiesExpression` | `rotor_tsrs::ast::Kind::SatisfiesExpression` |
| `ast` | const | `KindShorthandPropertyAssignment` | 1 (transformer) | `Kind::ShorthandPropertyAssignment` | `rotor_tsrs::ast::Kind::ShorthandPropertyAssignment` |
| `ast` | const | `KindSingleLineCommentTrivia` | 3 (flamework,transformer) | `Kind::SingleLineCommentTrivia` | `rotor_tsrs::ast::Kind::SingleLineCommentTrivia` |
| `ast` | const | `KindSlashEqualsToken` | 2 (flamework,transformer) | `Kind::SlashEqualsToken` | `rotor_tsrs::ast::Kind::SlashEqualsToken` |
| `ast` | const | `KindSlashToken` | 3 (flamework,transformer) | `Kind::SlashToken` | `rotor_tsrs::ast::Kind::SlashToken` |
| `ast` | const | `KindSourceFile` | 3 (jecs,transformer) | `Kind::SourceFile` | `rotor_tsrs::ast::Kind::SourceFile` |
| `ast` | const | `KindSpreadAssignment` | 1 (transformer) | `Kind::SpreadAssignment` | `rotor_tsrs::ast::Kind::SpreadAssignment` |
| `ast` | const | `KindSpreadElement` | 1 (transformer) | `Kind::SpreadElement` | `rotor_tsrs::ast::Kind::SpreadElement` |
| `ast` | const | `KindStringKeyword` | 1 (transformer) | `Kind::StringKeyword` | `rotor_tsrs::ast::Kind::StringKeyword` |
| `ast` | const | `KindStringLiteral` | 8 (flamework,transformer) | `Kind::StringLiteral` | `rotor_tsrs::ast::Kind::StringLiteral` |
| `ast` | const | `KindSuperKeyword` | 1 (transformer) | `Kind::SuperKeyword` | `rotor_tsrs::ast::Kind::SuperKeyword` |
| `ast` | const | `KindSwitchStatement` | 1 (transformer) | `Kind::SwitchStatement` | `rotor_tsrs::ast::Kind::SwitchStatement` |
| `ast` | const | `KindSymbolKeyword` | 1 (transformer) | `Kind::SymbolKeyword` | `rotor_tsrs::ast::Kind::SymbolKeyword` |
| `ast` | const | `KindTaggedTemplateExpression` | 1 (transformer) | `Kind::TaggedTemplateExpression` | `rotor_tsrs::ast::Kind::TaggedTemplateExpression` |
| `ast` | const | `KindTemplateExpression` | 1 (transformer) | `Kind::TemplateExpression` | `rotor_tsrs::ast::Kind::TemplateExpression` |
| `ast` | const | `KindTemplateHead` | 1 (transformer) | `Kind::TemplateHead` | `rotor_tsrs::ast::Kind::TemplateHead` |
| `ast` | const | `KindTemplateLiteralType` | 3 (transformer) | `Kind::TemplateLiteralType` | `rotor_tsrs::ast::Kind::TemplateLiteralType` |
| `ast` | const | `KindTemplateMiddle` | 1 (transformer) | `Kind::TemplateMiddle` | `rotor_tsrs::ast::Kind::TemplateMiddle` |
| `ast` | const | `KindTemplateTail` | 1 (transformer) | `Kind::TemplateTail` | `rotor_tsrs::ast::Kind::TemplateTail` |
| `ast` | const | `KindThisKeyword` | 2 (transformer) | `Kind::ThisKeyword` | `rotor_tsrs::ast::Kind::ThisKeyword` |
| `ast` | const | `KindThisType` | 1 (transformer) | `Kind::ThisType` | `rotor_tsrs::ast::Kind::ThisType` |
| `ast` | const | `KindThrowStatement` | 2 (transformer) | `Kind::ThrowStatement` | `rotor_tsrs::ast::Kind::ThrowStatement` |
| `ast` | const | `KindTildeToken` | 1 (transformer) | `Kind::TildeToken` | `rotor_tsrs::ast::Kind::TildeToken` |
| `ast` | const | `KindTrueKeyword` | 11 (flamework,jecs,transformer) | `Kind::TrueKeyword` | `rotor_tsrs::ast::Kind::TrueKeyword` |
| `ast` | const | `KindTryStatement` | 1 (transformer) | `Kind::TryStatement` | `rotor_tsrs::ast::Kind::TryStatement` |
| `ast` | const | `KindTupleType` | 2 (transformer) | `Kind::TupleType` | `rotor_tsrs::ast::Kind::TupleType` |
| `ast` | const | `KindTypeAliasDeclaration` | 1 (transformer) | `Kind::TypeAliasDeclaration` | `rotor_tsrs::ast::Kind::TypeAliasDeclaration` |
| `ast` | const | `KindTypeAssertionExpression` | 3 (transformer) | `Kind::TypeAssertionExpression` | `rotor_tsrs::ast::Kind::TypeAssertionExpression` |
| `ast` | const | `KindTypeKeyword` | 1 (flamework) | `Kind::TypeKeyword` | `rotor_tsrs::ast::Kind::TypeKeyword` |
| `ast` | const | `KindTypeLiteral` | 1 (transformer) | `Kind::TypeLiteral` | `rotor_tsrs::ast::Kind::TypeLiteral` |
| `ast` | const | `KindTypeOfExpression` | 1 (transformer) | `Kind::TypeOfExpression` | `rotor_tsrs::ast::Kind::TypeOfExpression` |
| `ast` | const | `KindTypeOperator` | 4 (transformer) | `Kind::TypeOperator` | `rotor_tsrs::ast::Kind::TypeOperator` |
| `ast` | const | `KindTypeParameter` | 1 (transformer) | `Kind::TypeParameter` | `rotor_tsrs::ast::Kind::TypeParameter` |
| `ast` | const | `KindTypeQuery` | 2 (transformer) | `Kind::TypeQuery` | `rotor_tsrs::ast::Kind::TypeQuery` |
| `ast` | const | `KindTypeReference` | 3 (transformer) | `Kind::TypeReference` | `rotor_tsrs::ast::Kind::TypeReference` |
| `ast` | const | `KindUndefinedKeyword` | 2 (transformer) | `Kind::UndefinedKeyword` | `rotor_tsrs::ast::Kind::UndefinedKeyword` |
| `ast` | const | `KindUnionType` | 1 (transformer) | `Kind::UnionType` | `rotor_tsrs::ast::Kind::UnionType` |
| `ast` | const | `KindUnknown` | 4 (flamework) | `Kind::Unknown` | `rotor_tsrs::ast::Kind::Unknown` |
| `ast` | const | `KindUnknownKeyword` | 3 (jecs,transformer) | `Kind::UnknownKeyword` | `rotor_tsrs::ast::Kind::UnknownKeyword` |
| `ast` | const | `KindVariableStatement` | 1 (transformer) | `Kind::VariableStatement` | `rotor_tsrs::ast::Kind::VariableStatement` |
| `ast` | const | `KindVoidExpression` | 1 (transformer) | `Kind::VoidExpression` | `rotor_tsrs::ast::Kind::VoidExpression` |
| `ast` | const | `KindVoidKeyword` | 2 (transformer) | `Kind::VoidKeyword` | `rotor_tsrs::ast::Kind::VoidKeyword` |
| `ast` | const | `KindWhileStatement` | 1 (transformer) | `Kind::WhileStatement` | `rotor_tsrs::ast::Kind::WhileStatement` |
| `ast` | const | `KindYieldExpression` | 1 (transformer) | `Kind::YieldExpression` | `rotor_tsrs::ast::Kind::YieldExpression` |
| `ast` | const | `ModifierFlagsAmbient` | 5 (transformer) | `ModifierFlags::Ambient` | `rotor_tsrs::ast::ModifierFlags::Ambient` |
| `ast` | const | `ModifierFlagsAsync` | 7 (transformer) | `ModifierFlags::Async` | `rotor_tsrs::ast::ModifierFlags::Async` |
| `ast` | const | `ModifierFlagsConst` | 2 (transformer) | `ModifierFlags::Const` | `rotor_tsrs::ast::ModifierFlags::Const` |
| `ast` | const | `ModifierFlagsExport` | 4 (transformer) | `ModifierFlags::Export` | `rotor_tsrs::ast::ModifierFlags::Export` |
| `ast` | const | `ModifierFlagsExportDefault` | 2 (transformer) | `ModifierFlags::ExportDefault` | `rotor_tsrs::ast::ModifierFlags::ExportDefault` |
| `ast` | const | `ModifierFlagsPrivate` | 2 (transformer) | `ModifierFlags::Private` | `rotor_tsrs::ast::ModifierFlags::Private` |
| `ast` | const | `ModifierFlagsProtected` | 2 (transformer) | `ModifierFlags::Protected` | `rotor_tsrs::ast::ModifierFlags::Protected` |
| `ast` | const | `ModifierFlagsReadonly` | 3 (transformer) | `ModifierFlags::Readonly` | `rotor_tsrs::ast::ModifierFlags::Readonly` |
| `ast` | const | `NodeFlagsConst` | 12 (flamework,jecs,transformer) | `NodeFlags::Const` | `rotor_tsrs::ast::NodeFlags::Const` |
| `ast` | const | `NodeFlagsExportContext` | 1 (transformer) | `NodeFlags::ExportContext` | `rotor_tsrs::ast::NodeFlags::ExportContext` |
| `ast` | const | `NodeFlagsLet` | 8 (jecs,transformer) | `NodeFlags::Let` | `rotor_tsrs::ast::NodeFlags::Let` |
| `ast` | const | `NodeFlagsNone` | 26 (flamework,jecs) | `NodeFlags::None` | `rotor_tsrs::ast::NodeFlags::None` |
| `ast` | const | `NodeFlagsOptionalChain` | 1 (jecs) | `NodeFlags::OptionalChain` | `rotor_tsrs::ast::NodeFlags::OptionalChain` |
| `ast` | const | `NodeFlagsUsing` | 1 (transformer) | `NodeFlags::Using` | `rotor_tsrs::ast::NodeFlags::Using` |
| `ast` | const | `SymbolFlagsAccessor` | 1 (transformer) | `SymbolFlags::Accessor` | `rotor_tsrs::ast::SymbolFlags::Accessor` |
| `ast` | const | `SymbolFlagsAlias` | 14 (flamework,jecs,transformer) | `SymbolFlags::Alias` | `rotor_tsrs::ast::SymbolFlags::Alias` |
| `ast` | const | `SymbolFlagsAll` | 3 (transformer) | `SymbolFlags::All` | `rotor_tsrs::ast::SymbolFlags::All` |
| `ast` | const | `SymbolFlagsClass` | 13 (flamework,transformer) | `SymbolFlags::Class` | `rotor_tsrs::ast::SymbolFlags::Class` |
| `ast` | const | `SymbolFlagsConstEnum` | 2 (transformer) | `SymbolFlags::ConstEnum` | `rotor_tsrs::ast::SymbolFlags::ConstEnum` |
| `ast` | const | `SymbolFlagsEnum` | 7 (transformer) | `SymbolFlags::Enum` | `rotor_tsrs::ast::SymbolFlags::Enum` |
| `ast` | const | `SymbolFlagsEnumMember` | 2 (transformer) | `SymbolFlags::EnumMember` | `rotor_tsrs::ast::SymbolFlags::EnumMember` |
| `ast` | const | `SymbolFlagsFunction` | 2 (transformer) | `SymbolFlags::Function` | `rotor_tsrs::ast::SymbolFlags::Function` |
| `ast` | const | `SymbolFlagsInterface` | 9 (flamework,transformer) | `SymbolFlags::Interface` | `rotor_tsrs::ast::SymbolFlags::Interface` |
| `ast` | const | `SymbolFlagsMethod` | 4 (transformer) | `SymbolFlags::Method` | `rotor_tsrs::ast::SymbolFlags::Method` |
| `ast` | const | `SymbolFlagsModule` | 4 (transformer) | `SymbolFlags::Module` | `rotor_tsrs::ast::SymbolFlags::Module` |
| `ast` | const | `SymbolFlagsOptional` | 3 (transformer) | `SymbolFlags::Optional` | `rotor_tsrs::ast::SymbolFlags::Optional` |
| `ast` | const | `SymbolFlagsProperty` | 1 (transformer) | `SymbolFlags::Property` | `rotor_tsrs::ast::SymbolFlags::Property` |
| `ast` | const | `SymbolFlagsPrototype` | 1 (transformer) | `SymbolFlags::Prototype` | `rotor_tsrs::ast::SymbolFlags::Prototype` |
| `ast` | const | `SymbolFlagsSetAccessor` | 1 (transformer) | `SymbolFlags::SetAccessor` | `rotor_tsrs::ast::SymbolFlags::SetAccessor` |
| `ast` | const | `SymbolFlagsType` | 6 (flamework,transformer) | `SymbolFlags::Type` | `rotor_tsrs::ast::SymbolFlags::Type` |
| `ast` | const | `SymbolFlagsTypeAlias` | 6 (flamework,transformer) | `SymbolFlags::TypeAlias` | `rotor_tsrs::ast::SymbolFlags::TypeAlias` |
| `ast` | const | `SymbolFlagsTypeParameter` | 4 (flamework,transformer) | `SymbolFlags::TypeParameter` | `rotor_tsrs::ast::SymbolFlags::TypeParameter` |
| `ast` | const | `SymbolFlagsValue` | 3 (jecs,transformer) | `SymbolFlags::Value` | `rotor_tsrs::ast::SymbolFlags::Value` |
| `ast` | const | `SymbolFlagsValueModule` | 1 (transformer) | `SymbolFlags::ValueModule` | `rotor_tsrs::ast::SymbolFlags::ValueModule` |
| `ast` | const | `SymbolFlagsVariable` | 2 (transformer) | `SymbolFlags::Variable` | `rotor_tsrs::ast::SymbolFlags::Variable` |
| `ast` | const | `TokenFlagsNone` | 49 (flamework) | `TokenFlags::None` | `rotor_tsrs::ast::TokenFlags::None` |
| `ast` | field | `ArrayLiteralExpression.Elements` | 10 (flamework,transformer) | `ArrayLiteralExpression::elements` | `rotor_tsrs::ast::ArrayLiteralExpression::elements` |
| `ast` | field | `ArrayTypeNode.ElementType` | 3 (transformer) | `ArrayTypeNode::element_type` | `rotor_tsrs::ast::ArrayTypeNode::element_type` |
| `ast` | field | `AwaitExpression.Expression` | 1 (transformer) | `AwaitExpression::expression` | `rotor_tsrs::ast::AwaitExpression::expression` |
| `ast` | field | `BinaryExpression.Left` | 40 (flamework,transformer) | `BinaryExpression::left` | `rotor_tsrs::ast::BinaryExpression::left` |
| `ast` | field | `BinaryExpression.OperatorToken` | 19 (flamework,transformer) | `BinaryExpression::operator_token` | `rotor_tsrs::ast::BinaryExpression::operator_token` |
| `ast` | field | `BinaryExpression.Right` | 31 (flamework,transformer) | `BinaryExpression::right` | `rotor_tsrs::ast::BinaryExpression::right` |
| `ast` | field | `BinaryOperatorToken.Kind` | 11 (flamework,transformer) | `BinaryOperatorToken::kind` | `rotor_tsrs::ast::BinaryOperatorToken::kind` |
| `ast` | field | `BindingElement.DotDotDotToken` | 6 (transformer) | `BindingElement::dot_dot_dot_token` | `rotor_tsrs::ast::BindingElement::dot_dot_dot_token` |
| `ast` | field | `BindingElement.Initializer` | 20 (transformer) | `BindingElement::initializer` | `rotor_tsrs::ast::BindingElement::initializer` |
| `ast` | field | `BindingElement.PropertyName` | 1 (transformer) | `BindingElement::property_name` | `rotor_tsrs::ast::BindingElement::property_name` |
| `ast` | field | `BindingElementList.Nodes` | 14 (jecs,transformer) | `BindingElementList::nodes` | `rotor_tsrs::ast::BindingElementList::nodes` |
| `ast` | field | `BindingPattern.Elements` | 14 (jecs,transformer) | `BindingPattern::elements` | `rotor_tsrs::ast::BindingPattern::elements` |
| `ast` | field | `Block.Statements` | 11 (transformer) | `Block::statements` | `rotor_tsrs::ast::Block::statements` |
| `ast` | field | `BreakStatement.Label` | 2 (transformer) | `BreakStatement::label` | `rotor_tsrs::ast::BreakStatement::label` |
| `ast` | field | `CallExpression.Arguments` | 14 (compile,flamework,jecs,transformer) | `CallExpression::arguments` | `rotor_tsrs::ast::CallExpression::arguments` |
| `ast` | field | `CallExpression.Expression` | 28 (compile,flamework,jecs,transformer) | `CallExpression::expression` | `rotor_tsrs::ast::CallExpression::expression` |
| `ast` | field | `CallExpression.Flags` | 2 (flamework) | `CallExpression::flags` | `rotor_tsrs::ast::CallExpression::flags` |
| `ast` | field | `CallExpression.QuestionDotToken` | 9 (flamework,transformer) | `CallExpression::question_dot_token` | `rotor_tsrs::ast::CallExpression::question_dot_token` |
| `ast` | field | `CallExpression.TypeArguments` | 5 (flamework,transformer) | `CallExpression::type_arguments` | `rotor_tsrs::ast::CallExpression::type_arguments` |
| `ast` | field | `CaseBlock.Clauses` | 3 (transformer) | `CaseBlock::clauses` | `rotor_tsrs::ast::CaseBlock::clauses` |
| `ast` | field | `CaseClausesList.Nodes` | 3 (transformer) | `CaseClausesList::nodes` | `rotor_tsrs::ast::CaseClausesList::nodes` |
| `ast` | field | `CaseOrDefaultClause.Expression` | 3 (transformer) | `CaseOrDefaultClause::expression` | `rotor_tsrs::ast::CaseOrDefaultClause::expression` |
| `ast` | field | `CaseOrDefaultClause.Statements` | 5 (transformer) | `CaseOrDefaultClause::statements` | `rotor_tsrs::ast::CaseOrDefaultClause::statements` |
| `ast` | field | `CatchClause.Block` | 3 (transformer) | `CatchClause::block` | `rotor_tsrs::ast::CatchClause::block` |
| `ast` | field | `CatchClause.VariableDeclaration` | 2 (transformer) | `CatchClause::variable_declaration` | `rotor_tsrs::ast::CatchClause::variable_declaration` |
| `ast` | field | `ClassDeclaration.HeritageClauses` | 2 (flamework) | `ClassDeclaration::heritage_clauses` | `rotor_tsrs::ast::ClassDeclaration::heritage_clauses` |
| `ast` | field | `ClassDeclaration.Members` | 5 (flamework) | `ClassDeclaration::members` | `rotor_tsrs::ast::ClassDeclaration::members` |
| `ast` | field | `ClassDeclaration.TypeParameters` | 1 (flamework) | `ClassDeclaration::type_parameters` | `rotor_tsrs::ast::ClassDeclaration::type_parameters` |
| `ast` | field | `ClassElementList.Nodes` | 5 (flamework) | `ClassElementList::nodes` | `rotor_tsrs::ast::ClassElementList::nodes` |
| `ast` | field | `ClassStaticBlockDeclaration.Body` | 1 (transformer) | `ClassStaticBlockDeclaration::body` | `rotor_tsrs::ast::ClassStaticBlockDeclaration::body` |
| `ast` | field | `CommentDirective.Loc` | 1 (compile) | `CommentDirective::loc` | `rotor_tsrs::ast::CommentDirective::loc` |
| `ast` | field | `CommentRange.Kind` | 2 (transformer) | `CommentRange::kind` | `rotor_tsrs::ast::CommentRange::kind` |
| `ast` | field | `ComputedPropertyName.Expression` | 3 (transformer) | `ComputedPropertyName::expression` | `rotor_tsrs::ast::ComputedPropertyName::expression` |
| `ast` | field | `ConditionalExpression.Condition` | 7 (transformer) | `ConditionalExpression::condition` | `rotor_tsrs::ast::ConditionalExpression::condition` |
| `ast` | field | `ConditionalExpression.WhenFalse` | 1 (transformer) | `ConditionalExpression::when_false` | `rotor_tsrs::ast::ConditionalExpression::when_false` |
| `ast` | field | `ConditionalExpression.WhenTrue` | 1 (transformer) | `ConditionalExpression::when_true` | `rotor_tsrs::ast::ConditionalExpression::when_true` |
| `ast` | field | `ConditionalTypeNode.CheckType` | 2 (transformer) | `ConditionalTypeNode::check_type` | `rotor_tsrs::ast::ConditionalTypeNode::check_type` |
| `ast` | field | `ConditionalTypeNode.ExtendsType` | 2 (transformer) | `ConditionalTypeNode::extends_type` | `rotor_tsrs::ast::ConditionalTypeNode::extends_type` |
| `ast` | field | `ConditionalTypeNode.FalseType` | 4 (flamework,transformer) | `ConditionalTypeNode::false_type` | `rotor_tsrs::ast::ConditionalTypeNode::false_type` |
| `ast` | field | `ConditionalTypeNode.TrueType` | 5 (flamework,transformer) | `ConditionalTypeNode::true_type` | `rotor_tsrs::ast::ConditionalTypeNode::true_type` |
| `ast` | field | `ContinueStatement.Label` | 2 (transformer) | `ContinueStatement::label` | `rotor_tsrs::ast::ContinueStatement::label` |
| `ast` | field | `DeclarationName.Kind` | 2 (transformer) | `DeclarationName::kind` | `rotor_tsrs::ast::DeclarationName::kind` |
| `ast` | field | `Decorator.Expression` | 3 (flamework) | `Decorator::expression` | `rotor_tsrs::ast::Decorator::expression` |
| `ast` | field | `DeleteExpression.Expression` | 2 (flamework,transformer) | `DeleteExpression::expression` | `rotor_tsrs::ast::DeleteExpression::expression` |
| `ast` | field | `DoStatement.Expression` | 1 (transformer) | `DoStatement::expression` | `rotor_tsrs::ast::DoStatement::expression` |
| `ast` | field | `DoStatement.Statement` | 2 (transformer) | `DoStatement::statement` | `rotor_tsrs::ast::DoStatement::statement` |
| `ast` | field | `ElementAccessExpression.ArgumentExpression` | 14 (compile,flamework,transformer) | `ElementAccessExpression::argument_expression` | `rotor_tsrs::ast::ElementAccessExpression::argument_expression` |
| `ast` | field | `ElementAccessExpression.Expression` | 12 (transformer) | `ElementAccessExpression::expression` | `rotor_tsrs::ast::ElementAccessExpression::expression` |
| `ast` | field | `ElementAccessExpression.Flags` | 1 (flamework) | `ElementAccessExpression::flags` | `rotor_tsrs::ast::ElementAccessExpression::flags` |
| `ast` | field | `ElementAccessExpression.QuestionDotToken` | 4 (flamework,transformer) | `ElementAccessExpression::question_dot_token` | `rotor_tsrs::ast::ElementAccessExpression::question_dot_token` |
| `ast` | field | `ElementList.Nodes` | 31 (compile,flamework,jecs,transformer) | `ElementList::nodes` | `rotor_tsrs::ast::ElementList::nodes` |
| `ast` | field | `EnumDeclaration.Members` | 4 (flamework,transformer) | `EnumDeclaration::members` | `rotor_tsrs::ast::EnumDeclaration::members` |
| `ast` | field | `EnumMember.Initializer` | 1 (transformer) | `EnumMember::initializer` | `rotor_tsrs::ast::EnumMember::initializer` |
| `ast` | field | `EnumMemberList.Nodes` | 4 (flamework,transformer) | `EnumMemberList::nodes` | `rotor_tsrs::ast::EnumMemberList::nodes` |
| `ast` | field | `ExportAssignment.Expression` | 4 (transformer) | `ExportAssignment::expression` | `rotor_tsrs::ast::ExportAssignment::expression` |
| `ast` | field | `ExportAssignment.IsExportEquals` | 2 (transformer) | `ExportAssignment::is_export_equals` | `rotor_tsrs::ast::ExportAssignment::is_export_equals` |
| `ast` | field | `ExportDeclaration.ExportClause` | 5 (transformer) | `ExportDeclaration::export_clause` | `rotor_tsrs::ast::ExportDeclaration::export_clause` |
| `ast` | field | `ExportDeclaration.IsTypeOnly` | 1 (transformer) | `ExportDeclaration::is_type_only` | `rotor_tsrs::ast::ExportDeclaration::is_type_only` |
| `ast` | field | `ExportDeclaration.ModuleSpecifier` | 10 (compile,transformer) | `ExportDeclaration::module_specifier` | `rotor_tsrs::ast::ExportDeclaration::module_specifier` |
| `ast` | field | `ExportSpecifier.IsTypeOnly` | 1 (transformer) | `ExportSpecifier::is_type_only` | `rotor_tsrs::ast::ExportSpecifier::is_type_only` |
| `ast` | field | `ExportSpecifier.PropertyName` | 1 (transformer) | `ExportSpecifier::property_name` | `rotor_tsrs::ast::ExportSpecifier::property_name` |
| `ast` | field | `ExportSpecifierList.Nodes` | 3 (transformer) | `ExportSpecifierList::nodes` | `rotor_tsrs::ast::ExportSpecifierList::nodes` |
| `ast` | field | `Expression.Kind` | 3 (compile,transformer) | `Expression::kind` | `rotor_tsrs::ast::Expression::kind` |
| `ast` | field | `ExpressionStatement.Expression` | 1 (transformer) | `ExpressionStatement::expression` | `rotor_tsrs::ast::ExpressionStatement::expression` |
| `ast` | field | `ExpressionWithTypeArguments.Expression` | 2 (flamework) | `ExpressionWithTypeArguments::expression` | `rotor_tsrs::ast::ExpressionWithTypeArguments::expression` |
| `ast` | field | `ExpressionWithTypeArgumentsList.Nodes` | 1 (flamework) | `ExpressionWithTypeArgumentsList::nodes` | `rotor_tsrs::ast::ExpressionWithTypeArgumentsList::nodes` |
| `ast` | field | `ExternalModuleReference.Expression` | 2 (compile,transformer) | `ExternalModuleReference::expression` | `rotor_tsrs::ast::ExternalModuleReference::expression` |
| `ast` | field | `FileReference.FileName` | 1 (compile) | `FileReference::file_name` | `rotor_tsrs::ast::FileReference::file_name` |
| `ast` | field | `ForInOrOfStatement.AwaitModifier` | 1 (transformer) | `ForInOrOfStatement::await_modifier` | `rotor_tsrs::ast::ForInOrOfStatement::await_modifier` |
| `ast` | field | `ForInOrOfStatement.Expression` | 12 (jecs,transformer) | `ForInOrOfStatement::expression` | `rotor_tsrs::ast::ForInOrOfStatement::expression` |
| `ast` | field | `ForInOrOfStatement.Initializer` | 10 (jecs,transformer) | `ForInOrOfStatement::initializer` | `rotor_tsrs::ast::ForInOrOfStatement::initializer` |
| `ast` | field | `ForInOrOfStatement.Statement` | 5 (jecs,transformer) | `ForInOrOfStatement::statement` | `rotor_tsrs::ast::ForInOrOfStatement::statement` |
| `ast` | field | `ForInitializer.Flags` | 2 (jecs,transformer) | `ForInitializer::flags` | `rotor_tsrs::ast::ForInitializer::flags` |
| `ast` | field | `ForStatement.Condition` | 4 (transformer) | `ForStatement::condition` | `rotor_tsrs::ast::ForStatement::condition` |
| `ast` | field | `ForStatement.Incrementor` | 3 (transformer) | `ForStatement::incrementor` | `rotor_tsrs::ast::ForStatement::incrementor` |
| `ast` | field | `ForStatement.Initializer` | 2 (transformer) | `ForStatement::initializer` | `rotor_tsrs::ast::ForStatement::initializer` |
| `ast` | field | `ForStatement.Statement` | 2 (transformer) | `ForStatement::statement` | `rotor_tsrs::ast::ForStatement::statement` |
| `ast` | field | `FunctionDeclaration.AsteriskToken` | 2 (transformer) | `FunctionDeclaration::asterisk_token` | `rotor_tsrs::ast::FunctionDeclaration::asterisk_token` |
| `ast` | field | `FunctionDeclaration.Body` | 2 (transformer) | `FunctionDeclaration::body` | `rotor_tsrs::ast::FunctionDeclaration::body` |
| `ast` | field | `FunctionExpression.AsteriskToken` | 3 (transformer) | `FunctionExpression::asterisk_token` | `rotor_tsrs::ast::FunctionExpression::asterisk_token` |
| `ast` | field | `FunctionTypeNode.Type` | 2 (transformer) | `FunctionTypeNode::type_` | `rotor_tsrs::ast::FunctionTypeNode::type_` |
| `ast` | field | `GetAccessorDeclaration.Body` | 1 (flamework) | `GetAccessorDeclaration::body` | `rotor_tsrs::ast::GetAccessorDeclaration::body` |
| `ast` | field | `GetAccessorDeclaration.FullSignature` | 1 (flamework) | `GetAccessorDeclaration::full_signature` | `rotor_tsrs::ast::GetAccessorDeclaration::full_signature` |
| `ast` | field | `GetAccessorDeclaration.Parameters` | 1 (flamework) | `GetAccessorDeclaration::parameters` | `rotor_tsrs::ast::GetAccessorDeclaration::parameters` |
| `ast` | field | `GetAccessorDeclaration.Type` | 1 (flamework) | `GetAccessorDeclaration::type_` | `rotor_tsrs::ast::GetAccessorDeclaration::type_` |
| `ast` | field | `GetAccessorDeclaration.TypeParameters` | 1 (flamework) | `GetAccessorDeclaration::type_parameters` | `rotor_tsrs::ast::GetAccessorDeclaration::type_parameters` |
| `ast` | field | `HeritageClause.Token` | 1 (flamework) | `HeritageClause::token` | `rotor_tsrs::ast::HeritageClause::token` |
| `ast` | field | `HeritageClause.Types` | 1 (flamework) | `HeritageClause::types` | `rotor_tsrs::ast::HeritageClause::types` |
| `ast` | field | `HeritageClauseList.Nodes` | 1 (flamework) | `HeritageClauseList::nodes` | `rotor_tsrs::ast::HeritageClauseList::nodes` |
| `ast` | field | `IfStatement.ElseStatement` | 2 (transformer) | `IfStatement::else_statement` | `rotor_tsrs::ast::IfStatement::else_statement` |
| `ast` | field | `IfStatement.Expression` | 3 (transformer) | `IfStatement::expression` | `rotor_tsrs::ast::IfStatement::expression` |
| `ast` | field | `IfStatement.ThenStatement` | 3 (transformer) | `IfStatement::then_statement` | `rotor_tsrs::ast::IfStatement::then_statement` |
| `ast` | field | `ImportClause.NamedBindings` | 18 (flamework,transformer) | `ImportClause::named_bindings` | `rotor_tsrs::ast::ImportClause::named_bindings` |
| `ast` | field | `ImportClause.PhaseModifier` | 4 (flamework) | `ImportClause::phase_modifier` | `rotor_tsrs::ast::ImportClause::phase_modifier` |
| `ast` | field | `ImportDeclaration.Attributes` | 1 (flamework) | `ImportDeclaration::attributes` | `rotor_tsrs::ast::ImportDeclaration::attributes` |
| `ast` | field | `ImportDeclaration.ImportClause` | 13 (flamework,transformer) | `ImportDeclaration::import_clause` | `rotor_tsrs::ast::ImportDeclaration::import_clause` |
| `ast` | field | `ImportDeclaration.ModuleSpecifier` | 13 (compile,flamework,transformer) | `ImportDeclaration::module_specifier` | `rotor_tsrs::ast::ImportDeclaration::module_specifier` |
| `ast` | field | `ImportEqualsDeclaration.ModuleReference` | 1 (transformer) | `ImportEqualsDeclaration::module_reference` | `rotor_tsrs::ast::ImportEqualsDeclaration::module_reference` |
| `ast` | field | `ImportSpecifier.IsTypeOnly` | 5 (flamework) | `ImportSpecifier::is_type_only` | `rotor_tsrs::ast::ImportSpecifier::is_type_only` |
| `ast` | field | `ImportSpecifier.PropertyName` | 10 (flamework,transformer) | `ImportSpecifier::property_name` | `rotor_tsrs::ast::ImportSpecifier::property_name` |
| `ast` | field | `ImportSpecifierList.Nodes` | 7 (flamework,transformer) | `ImportSpecifierList::nodes` | `rotor_tsrs::ast::ImportSpecifierList::nodes` |
| `ast` | field | `ImportTypeNode.Argument` | 1 (compile) | `ImportTypeNode::argument` | `rotor_tsrs::ast::ImportTypeNode::argument` |
| `ast` | field | `IndexSignatureDeclaration.Type` | 1 (transformer) | `IndexSignatureDeclaration::type_` | `rotor_tsrs::ast::IndexSignatureDeclaration::type_` |
| `ast` | field | `IndexedAccessTypeNode.IndexType` | 2 (transformer) | `IndexedAccessTypeNode::index_type` | `rotor_tsrs::ast::IndexedAccessTypeNode::index_type` |
| `ast` | field | `IndexedAccessTypeNode.ObjectType` | 2 (transformer) | `IndexedAccessTypeNode::object_type` | `rotor_tsrs::ast::IndexedAccessTypeNode::object_type` |
| `ast` | field | `InferTypeNode.TypeParameter` | 2 (transformer) | `InferTypeNode::type_parameter` | `rotor_tsrs::ast::InferTypeNode::type_parameter` |
| `ast` | field | `InterfaceDeclaration.Members` | 3 (transformer) | `InterfaceDeclaration::members` | `rotor_tsrs::ast::InterfaceDeclaration::members` |
| `ast` | field | `IntersectionTypeNode.Types` | 1 (transformer) | `IntersectionTypeNode::types` | `rotor_tsrs::ast::IntersectionTypeNode::types` |
| `ast` | field | `JSDoc.Tags` | 6 (flamework) | `JSDoc::tags` | `rotor_tsrs::ast::JSDoc::tags` |
| `ast` | field | `JsxAttribute.Initializer` | 1 (transformer) | `JsxAttribute::initializer` | `rotor_tsrs::ast::JsxAttribute::initializer` |
| `ast` | field | `JsxAttributeList.Nodes` | 2 (transformer) | `JsxAttributeList::nodes` | `rotor_tsrs::ast::JsxAttributeList::nodes` |
| `ast` | field | `JsxAttributes.Properties` | 2 (transformer) | `JsxAttributes::properties` | `rotor_tsrs::ast::JsxAttributes::properties` |
| `ast` | field | `JsxChildList.Nodes` | 2 (transformer) | `JsxChildList::nodes` | `rotor_tsrs::ast::JsxChildList::nodes` |
| `ast` | field | `JsxElement.Children` | 1 (transformer) | `JsxElement::children` | `rotor_tsrs::ast::JsxElement::children` |
| `ast` | field | `JsxElement.OpeningElement` | 1 (transformer) | `JsxElement::opening_element` | `rotor_tsrs::ast::JsxElement::opening_element` |
| `ast` | field | `JsxExpression.DotDotDotToken` | 2 (transformer) | `JsxExpression::dot_dot_dot_token` | `rotor_tsrs::ast::JsxExpression::dot_dot_dot_token` |
| `ast` | field | `JsxExpression.Expression` | 4 (transformer) | `JsxExpression::expression` | `rotor_tsrs::ast::JsxExpression::expression` |
| `ast` | field | `JsxFragment.Children` | 1 (transformer) | `JsxFragment::children` | `rotor_tsrs::ast::JsxFragment::children` |
| `ast` | field | `JsxNamespacedName.Namespace` | 1 (transformer) | `JsxNamespacedName::namespace` | `rotor_tsrs::ast::JsxNamespacedName::namespace` |
| `ast` | field | `JsxOpeningElement.Attributes` | 1 (transformer) | `JsxOpeningElement::attributes` | `rotor_tsrs::ast::JsxOpeningElement::attributes` |
| `ast` | field | `JsxOpeningElement.TagName` | 1 (transformer) | `JsxOpeningElement::tag_name` | `rotor_tsrs::ast::JsxOpeningElement::tag_name` |
| `ast` | field | `JsxSelfClosingElement.Attributes` | 1 (transformer) | `JsxSelfClosingElement::attributes` | `rotor_tsrs::ast::JsxSelfClosingElement::attributes` |
| `ast` | field | `JsxSelfClosingElement.TagName` | 1 (transformer) | `JsxSelfClosingElement::tag_name` | `rotor_tsrs::ast::JsxSelfClosingElement::tag_name` |
| `ast` | field | `JsxSpreadAttribute.Expression` | 1 (transformer) | `JsxSpreadAttribute::expression` | `rotor_tsrs::ast::JsxSpreadAttribute::expression` |
| `ast` | field | `JsxText.ContainsOnlyTriviaWhiteSpaces` | 1 (transformer) | `JsxText::contains_only_trivia_white_spaces` | `rotor_tsrs::ast::JsxText::contains_only_trivia_white_spaces` |
| `ast` | field | `JsxText.Text` | 1 (transformer) | `JsxText::text` | `rotor_tsrs::ast::JsxText::text` |
| `ast` | field | `LabeledStatement.Label` | 1 (transformer) | `LabeledStatement::label` | `rotor_tsrs::ast::LabeledStatement::label` |
| `ast` | field | `LabeledStatement.Statement` | 2 (transformer) | `LabeledStatement::statement` | `rotor_tsrs::ast::LabeledStatement::statement` |
| `ast` | field | `LiteralTypeNode.Literal` | 3 (compile,transformer) | `LiteralTypeNode::literal` | `rotor_tsrs::ast::LiteralTypeNode::literal` |
| `ast` | field | `MappedTypeNode.NameType` | 6 (transformer) | `MappedTypeNode::name_type` | `rotor_tsrs::ast::MappedTypeNode::name_type` |
| `ast` | field | `MappedTypeNode.QuestionToken` | 1 (transformer) | `MappedTypeNode::question_token` | `rotor_tsrs::ast::MappedTypeNode::question_token` |
| `ast` | field | `MappedTypeNode.ReadonlyToken` | 1 (transformer) | `MappedTypeNode::readonly_token` | `rotor_tsrs::ast::MappedTypeNode::readonly_token` |
| `ast` | field | `MappedTypeNode.Type` | 3 (transformer) | `MappedTypeNode::type_` | `rotor_tsrs::ast::MappedTypeNode::type_` |
| `ast` | field | `MappedTypeNode.TypeParameter` | 3 (transformer) | `MappedTypeNode::type_parameter` | `rotor_tsrs::ast::MappedTypeNode::type_parameter` |
| `ast` | field | `MethodDeclaration.AsteriskToken` | 3 (flamework,transformer) | `MethodDeclaration::asterisk_token` | `rotor_tsrs::ast::MethodDeclaration::asterisk_token` |
| `ast` | field | `MethodDeclaration.Body` | 4 (flamework,transformer) | `MethodDeclaration::body` | `rotor_tsrs::ast::MethodDeclaration::body` |
| `ast` | field | `MethodDeclaration.FullSignature` | 1 (flamework) | `MethodDeclaration::full_signature` | `rotor_tsrs::ast::MethodDeclaration::full_signature` |
| `ast` | field | `MethodDeclaration.Parameters` | 1 (flamework) | `MethodDeclaration::parameters` | `rotor_tsrs::ast::MethodDeclaration::parameters` |
| `ast` | field | `MethodDeclaration.PostfixToken` | 1 (flamework) | `MethodDeclaration::postfix_token` | `rotor_tsrs::ast::MethodDeclaration::postfix_token` |
| `ast` | field | `MethodDeclaration.Type` | 1 (flamework) | `MethodDeclaration::type_` | `rotor_tsrs::ast::MethodDeclaration::type_` |
| `ast` | field | `MethodDeclaration.TypeParameters` | 1 (flamework) | `MethodDeclaration::type_parameters` | `rotor_tsrs::ast::MethodDeclaration::type_parameters` |
| `ast` | field | `ModifierList.Nodes` | 7 (flamework,transformer) | `ModifierList::nodes` | `rotor_tsrs::ast::ModifierList::nodes` |
| `ast` | field | `ModuleBlock.Statements` | 1 (transformer) | `ModuleBlock::statements` | `rotor_tsrs::ast::ModuleBlock::statements` |
| `ast` | field | `ModuleDeclaration.Keyword` | 1 (transformer) | `ModuleDeclaration::keyword` | `rotor_tsrs::ast::ModuleDeclaration::keyword` |
| `ast` | field | `NamedExports.Elements` | 3 (transformer) | `NamedExports::elements` | `rotor_tsrs::ast::NamedExports::elements` |
| `ast` | field | `NamedImports.Elements` | 7 (flamework,transformer) | `NamedImports::elements` | `rotor_tsrs::ast::NamedImports::elements` |
| `ast` | field | `NamedTupleMember.Type` | 1 (transformer) | `NamedTupleMember::type_` | `rotor_tsrs::ast::NamedTupleMember::type_` |
| `ast` | field | `NewExpression.Arguments` | 7 (transformer) | `NewExpression::arguments` | `rotor_tsrs::ast::NewExpression::arguments` |
| `ast` | field | `NewExpression.Expression` | 5 (flamework,transformer) | `NewExpression::expression` | `rotor_tsrs::ast::NewExpression::expression` |
| `ast` | field | `NewExpression.TypeArguments` | 2 (flamework) | `NewExpression::type_arguments` | `rotor_tsrs::ast::NewExpression::type_arguments` |
| `ast` | field | `Node.Flags` | 5 (flamework,transformer) | `Node::flags` | `rotor_tsrs::ast::Node::flags` |
| `ast` | field | `Node.Kind` | 86 (compile,flamework,jecs,transformer) | `Node::kind` | `rotor_tsrs::ast::Node::kind` |
| `ast` | field | `Node.Loc` | 4 (flamework,jecs,transformer) | `Node::loc` | `rotor_tsrs::ast::Node::loc` |
| `ast` | field | `Node.Parent` | 132 (flamework,jecs,transformer) | `Node::parent` | `rotor_tsrs::ast::Node::parent` |
| `ast` | field | `NodeList.Loc` | 2 (jecs) | `NodeList::loc` | `rotor_tsrs::ast::NodeList::loc` |
| `ast` | field | `NodeList.Nodes` | 39 (compile,flamework,jecs,transformer) | `NodeList::nodes` | `rotor_tsrs::ast::NodeList::nodes` |
| `ast` | field | `ObjectLiteralExpression.MultiLine` | 1 (flamework) | `ObjectLiteralExpression::multi_line` | `rotor_tsrs::ast::ObjectLiteralExpression::multi_line` |
| `ast` | field | `ObjectLiteralExpression.Properties` | 12 (flamework,jecs,transformer) | `ObjectLiteralExpression::properties` | `rotor_tsrs::ast::ObjectLiteralExpression::properties` |
| `ast` | field | `ParameterDeclaration.DotDotDotToken` | 10 (transformer) | `ParameterDeclaration::dot_dot_dot_token` | `rotor_tsrs::ast::ParameterDeclaration::dot_dot_dot_token` |
| `ast` | field | `ParameterDeclaration.Initializer` | 4 (transformer) | `ParameterDeclaration::initializer` | `rotor_tsrs::ast::ParameterDeclaration::initializer` |
| `ast` | field | `ParameterDeclaration.QuestionToken` | 3 (transformer) | `ParameterDeclaration::question_token` | `rotor_tsrs::ast::ParameterDeclaration::question_token` |
| `ast` | field | `ParameterDeclaration.Type` | 8 (flamework,transformer) | `ParameterDeclaration::type_` | `rotor_tsrs::ast::ParameterDeclaration::type_` |
| `ast` | field | `ParameterDeclarationNode.Parent` | 1 (transformer) | `ParameterDeclarationNode::parent` | `rotor_tsrs::ast::ParameterDeclarationNode::parent` |
| `ast` | field | `ParenthesizedExpression.Expression` | 1 (transformer) | `ParenthesizedExpression::expression` | `rotor_tsrs::ast::ParenthesizedExpression::expression` |
| `ast` | field | `ParenthesizedTypeNode.Type` | 1 (transformer) | `ParenthesizedTypeNode::type_` | `rotor_tsrs::ast::ParenthesizedTypeNode::type_` |
| `ast` | field | `PostfixUnaryExpression.Operand` | 5 (flamework,transformer) | `PostfixUnaryExpression::operand` | `rotor_tsrs::ast::PostfixUnaryExpression::operand` |
| `ast` | field | `PostfixUnaryExpression.Operator` | 6 (flamework,transformer) | `PostfixUnaryExpression::operator` | `rotor_tsrs::ast::PostfixUnaryExpression::operator` |
| `ast` | field | `PrefixUnaryExpression.Operand` | 15 (flamework,transformer) | `PrefixUnaryExpression::operand` | `rotor_tsrs::ast::PrefixUnaryExpression::operand` |
| `ast` | field | `PrefixUnaryExpression.Operator` | 9 (flamework,transformer) | `PrefixUnaryExpression::operator` | `rotor_tsrs::ast::PrefixUnaryExpression::operator` |
| `ast` | field | `PropertyAccessExpression.Expression` | 14 (jecs,transformer) | `PropertyAccessExpression::expression` | `rotor_tsrs::ast::PropertyAccessExpression::expression` |
| `ast` | field | `PropertyAccessExpression.QuestionDotToken` | 3 (transformer) | `PropertyAccessExpression::question_dot_token` | `rotor_tsrs::ast::PropertyAccessExpression::question_dot_token` |
| `ast` | field | `PropertyAssignment.Initializer` | 5 (flamework,transformer) | `PropertyAssignment::initializer` | `rotor_tsrs::ast::PropertyAssignment::initializer` |
| `ast` | field | `PropertyAssignment.PostfixToken` | 2 (flamework) | `PropertyAssignment::postfix_token` | `rotor_tsrs::ast::PropertyAssignment::postfix_token` |
| `ast` | field | `PropertyAssignment.Type` | 2 (flamework) | `PropertyAssignment::type_` | `rotor_tsrs::ast::PropertyAssignment::type_` |
| `ast` | field | `PropertyDeclaration.Initializer` | 1 (flamework) | `PropertyDeclaration::initializer` | `rotor_tsrs::ast::PropertyDeclaration::initializer` |
| `ast` | field | `PropertyDeclaration.PostfixToken` | 1 (flamework) | `PropertyDeclaration::postfix_token` | `rotor_tsrs::ast::PropertyDeclaration::postfix_token` |
| `ast` | field | `PropertyDeclaration.Type` | 3 (flamework) | `PropertyDeclaration::type_` | `rotor_tsrs::ast::PropertyDeclaration::type_` |
| `ast` | field | `PropertySignatureDeclaration.PostfixToken` | 2 (transformer) | `PropertySignatureDeclaration::postfix_token` | `rotor_tsrs::ast::PropertySignatureDeclaration::postfix_token` |
| `ast` | field | `PropertySignatureDeclaration.Type` | 1 (transformer) | `PropertySignatureDeclaration::type_` | `rotor_tsrs::ast::PropertySignatureDeclaration::type_` |
| `ast` | field | `QualifiedName.Left` | 2 (transformer) | `QualifiedName::left` | `rotor_tsrs::ast::QualifiedName::left` |
| `ast` | field | `QualifiedName.Right` | 2 (transformer) | `QualifiedName::right` | `rotor_tsrs::ast::QualifiedName::right` |
| `ast` | field | `ReturnStatement.Expression` | 1 (transformer) | `ReturnStatement::expression` | `rotor_tsrs::ast::ReturnStatement::expression` |
| `ast` | field | `SetAccessorDeclaration.Body` | 1 (flamework) | `SetAccessorDeclaration::body` | `rotor_tsrs::ast::SetAccessorDeclaration::body` |
| `ast` | field | `SetAccessorDeclaration.FullSignature` | 1 (flamework) | `SetAccessorDeclaration::full_signature` | `rotor_tsrs::ast::SetAccessorDeclaration::full_signature` |
| `ast` | field | `SetAccessorDeclaration.Parameters` | 1 (flamework) | `SetAccessorDeclaration::parameters` | `rotor_tsrs::ast::SetAccessorDeclaration::parameters` |
| `ast` | field | `SetAccessorDeclaration.Type` | 1 (flamework) | `SetAccessorDeclaration::type_` | `rotor_tsrs::ast::SetAccessorDeclaration::type_` |
| `ast` | field | `SetAccessorDeclaration.TypeParameters` | 1 (flamework) | `SetAccessorDeclaration::type_parameters` | `rotor_tsrs::ast::SetAccessorDeclaration::type_parameters` |
| `ast` | field | `ShorthandPropertyAssignment.ObjectAssignmentInitializer` | 3 (transformer) | `ShorthandPropertyAssignment::object_assignment_initializer` | `rotor_tsrs::ast::ShorthandPropertyAssignment::object_assignment_initializer` |
| `ast` | field | `SourceFile.CommentDirectives` | 3 (compile) | `SourceFile::comment_directives` | `rotor_tsrs::ast::SourceFile::comment_directives` |
| `ast` | field | `SourceFile.EndOfFileToken` | 5 (flamework,jecs) | `SourceFile::end_of_file_token` | `rotor_tsrs::ast::SourceFile::end_of_file_token` |
| `ast` | field | `SourceFile.IsDeclarationFile` | 11 (compile,flamework,jecs) | `SourceFile::is_declaration_file` | `rotor_tsrs::ast::SourceFile::is_declaration_file` |
| `ast` | field | `SourceFile.ModuleAugmentations` | 3 (compile) | `SourceFile::module_augmentations` | `rotor_tsrs::ast::SourceFile::module_augmentations` |
| `ast` | field | `SourceFile.ReferencedFiles` | 1 (compile) | `SourceFile::referenced_files` | `rotor_tsrs::ast::SourceFile::referenced_files` |
| `ast` | field | `SourceFile.Statements` | 22 (compile,flamework,jecs,transformer) | `SourceFile::statements` | `rotor_tsrs::ast::SourceFile::statements` |
| `ast` | field | `SourceFile.Symbol` | 1 (jecs) | `SourceFile::symbol` | `rotor_tsrs::ast::SourceFile::symbol` |
| `ast` | field | `SourceFileParseOptions.FileName` | 9 (compile) | `SourceFileParseOptions::file_name` | `rotor_tsrs::ast::SourceFileParseOptions::file_name` |
| `ast` | field | `SourceFileParseOptions.Path` | 3 (compile) | `SourceFileParseOptions::path` | `rotor_tsrs::ast::SourceFileParseOptions::path` |
| `ast` | field | `SpreadAssignment.Expression` | 4 (transformer) | `SpreadAssignment::expression` | `rotor_tsrs::ast::SpreadAssignment::expression` |
| `ast` | field | `SpreadElement.Expression` | 6 (transformer) | `SpreadElement::expression` | `rotor_tsrs::ast::SpreadElement::expression` |
| `ast` | field | `StatementList.Nodes` | 17 (transformer) | `StatementList::nodes` | `rotor_tsrs::ast::StatementList::nodes` |
| `ast` | field | `SwitchStatement.CaseBlock` | 3 (transformer) | `SwitchStatement::case_block` | `rotor_tsrs::ast::SwitchStatement::case_block` |
| `ast` | field | `SwitchStatement.Expression` | 3 (transformer) | `SwitchStatement::expression` | `rotor_tsrs::ast::SwitchStatement::expression` |
| `ast` | field | `Symbol.CheckFlags` | 1 (transformer) | `Symbol::check_flags` | `rotor_tsrs::ast::Symbol::check_flags` |
| `ast` | field | `Symbol.Declarations` | 62 (compile,flamework,jecs,transformer) | `Symbol::declarations` | `rotor_tsrs::ast::Symbol::declarations` |
| `ast` | field | `Symbol.Exports` | 2 (flamework) | `Symbol::exports` | `rotor_tsrs::ast::Symbol::exports` |
| `ast` | field | `Symbol.Flags` | 63 (flamework,jecs,transformer) | `Symbol::flags` | `rotor_tsrs::ast::Symbol::flags` |
| `ast` | field | `Symbol.Members` | 2 (flamework) | `Symbol::members` | `rotor_tsrs::ast::Symbol::members` |
| `ast` | field | `Symbol.Name` | 97 (flamework,jecs,transformer) | `Symbol::name` | `rotor_tsrs::ast::Symbol::name` |
| `ast` | field | `Symbol.Parent` | 23 (flamework,transformer) | `Symbol::parent` | `rotor_tsrs::ast::Symbol::parent` |
| `ast` | field | `Symbol.ValueDeclaration` | 36 (flamework,jecs,transformer) | `Symbol::value_declaration` | `rotor_tsrs::ast::Symbol::value_declaration` |
| `ast` | field | `TaggedTemplateExpression.Tag` | 1 (transformer) | `TaggedTemplateExpression::tag` | `rotor_tsrs::ast::TaggedTemplateExpression::tag` |
| `ast` | field | `TaggedTemplateExpression.Template` | 3 (transformer) | `TaggedTemplateExpression::template` | `rotor_tsrs::ast::TaggedTemplateExpression::template` |
| `ast` | field | `TemplateExpression.Head` | 3 (transformer) | `TemplateExpression::head` | `rotor_tsrs::ast::TemplateExpression::head` |
| `ast` | field | `TemplateExpression.TemplateSpans` | 2 (transformer) | `TemplateExpression::template_spans` | `rotor_tsrs::ast::TemplateExpression::template_spans` |
| `ast` | field | `TemplateLiteralTypeNode.Head` | 1 (transformer) | `TemplateLiteralTypeNode::head` | `rotor_tsrs::ast::TemplateLiteralTypeNode::head` |
| `ast` | field | `TemplateLiteralTypeNode.TemplateSpans` | 1 (transformer) | `TemplateLiteralTypeNode::template_spans` | `rotor_tsrs::ast::TemplateLiteralTypeNode::template_spans` |
| `ast` | field | `TemplateLiteralTypeSpan.Literal` | 1 (transformer) | `TemplateLiteralTypeSpan::literal` | `rotor_tsrs::ast::TemplateLiteralTypeSpan::literal` |
| `ast` | field | `TemplateLiteralTypeSpan.Type` | 1 (transformer) | `TemplateLiteralTypeSpan::type_` | `rotor_tsrs::ast::TemplateLiteralTypeSpan::type_` |
| `ast` | field | `TemplateLiteralTypeSpanList.Nodes` | 1 (transformer) | `TemplateLiteralTypeSpanList::nodes` | `rotor_tsrs::ast::TemplateLiteralTypeSpanList::nodes` |
| `ast` | field | `TemplateSpan.Expression` | 2 (transformer) | `TemplateSpan::expression` | `rotor_tsrs::ast::TemplateSpan::expression` |
| `ast` | field | `TemplateSpan.Literal` | 2 (transformer) | `TemplateSpan::literal` | `rotor_tsrs::ast::TemplateSpan::literal` |
| `ast` | field | `TemplateSpanList.Nodes` | 2 (transformer) | `TemplateSpanList::nodes` | `rotor_tsrs::ast::TemplateSpanList::nodes` |
| `ast` | field | `ThrowStatement.Expression` | 1 (transformer) | `ThrowStatement::expression` | `rotor_tsrs::ast::ThrowStatement::expression` |
| `ast` | field | `TokenNode.Kind` | 1 (transformer) | `TokenNode::kind` | `rotor_tsrs::ast::TokenNode::kind` |
| `ast` | field | `TryStatement.CatchClause` | 4 (transformer) | `TryStatement::catch_clause` | `rotor_tsrs::ast::TryStatement::catch_clause` |
| `ast` | field | `TryStatement.FinallyBlock` | 5 (transformer) | `TryStatement::finally_block` | `rotor_tsrs::ast::TryStatement::finally_block` |
| `ast` | field | `TryStatement.TryBlock` | 4 (transformer) | `TryStatement::try_block` | `rotor_tsrs::ast::TryStatement::try_block` |
| `ast` | field | `TupleTypeNode.Elements` | 1 (transformer) | `TupleTypeNode::elements` | `rotor_tsrs::ast::TupleTypeNode::elements` |
| `ast` | field | `TypeAliasDeclaration.Type` | 5 (flamework,transformer) | `TypeAliasDeclaration::type_` | `rotor_tsrs::ast::TypeAliasDeclaration::type_` |
| `ast` | field | `TypeElementList.Nodes` | 3 (transformer) | `TypeElementList::nodes` | `rotor_tsrs::ast::TypeElementList::nodes` |
| `ast` | field | `TypeList.Nodes` | 5 (transformer) | `TypeList::nodes` | `rotor_tsrs::ast::TypeList::nodes` |
| `ast` | field | `TypeNode.Kind` | 4 (compile,transformer) | `TypeNode::kind` | `rotor_tsrs::ast::TypeNode::kind` |
| `ast` | field | `TypeOperatorNode.Operator` | 4 (transformer) | `TypeOperatorNode::operator` | `rotor_tsrs::ast::TypeOperatorNode::operator` |
| `ast` | field | `TypeOperatorNode.Type` | 6 (transformer) | `TypeOperatorNode::type_` | `rotor_tsrs::ast::TypeOperatorNode::type_` |
| `ast` | field | `TypeParameterDeclaration.Constraint` | 4 (flamework,transformer) | `TypeParameterDeclaration::constraint` | `rotor_tsrs::ast::TypeParameterDeclaration::constraint` |
| `ast` | field | `TypeQueryNode.ExprName` | 2 (flamework,transformer) | `TypeQueryNode::expr_name` | `rotor_tsrs::ast::TypeQueryNode::expr_name` |
| `ast` | field | `TypeReferenceNode.TypeName` | 6 (flamework,transformer) | `TypeReferenceNode::type_name` | `rotor_tsrs::ast::TypeReferenceNode::type_name` |
| `ast` | field | `UnionTypeNode.Types` | 1 (transformer) | `UnionTypeNode::types` | `rotor_tsrs::ast::UnionTypeNode::types` |
| `ast` | field | `VariableDeclaration.Initializer` | 9 (transformer) | `VariableDeclaration::initializer` | `rotor_tsrs::ast::VariableDeclaration::initializer` |
| `ast` | field | `VariableDeclaration.Type` | 3 (transformer) | `VariableDeclaration::type_` | `rotor_tsrs::ast::VariableDeclaration::type_` |
| `ast` | field | `VariableDeclarationList.Declarations` | 14 (jecs,transformer) | `VariableDeclarationList::declarations` | `rotor_tsrs::ast::VariableDeclarationList::declarations` |
| `ast` | field | `VariableDeclarationListNode.Flags` | 1 (transformer) | `VariableDeclarationListNode::flags` | `rotor_tsrs::ast::VariableDeclarationListNode::flags` |
| `ast` | field | `VariableDeclarationNodeList.Nodes` | 14 (jecs,transformer) | `VariableDeclarationNodeList::nodes` | `rotor_tsrs::ast::VariableDeclarationNodeList::nodes` |
| `ast` | field | `VariableStatement.DeclarationList` | 3 (transformer) | `VariableStatement::declaration_list` | `rotor_tsrs::ast::VariableStatement::declaration_list` |
| `ast` | field | `VoidExpression.Expression` | 1 (transformer) | `VoidExpression::expression` | `rotor_tsrs::ast::VoidExpression::expression` |
| `ast` | field | `WhileStatement.Expression` | 3 (transformer) | `WhileStatement::expression` | `rotor_tsrs::ast::WhileStatement::expression` |
| `ast` | field | `WhileStatement.Statement` | 2 (transformer) | `WhileStatement::statement` | `rotor_tsrs::ast::WhileStatement::statement` |
| `ast` | field | `YieldExpression.AsteriskToken` | 1 (transformer) | `YieldExpression::asterisk_token` | `rotor_tsrs::ast::YieldExpression::asterisk_token` |
| `ast` | field | `YieldExpression.Expression` | 2 (transformer) | `YieldExpression::expression` | `rotor_tsrs::ast::YieldExpression::expression` |
| `ast` | func | `CanHaveModifiers` | 1 (transformer) | `can_have_modifiers` | `rotor_tsrs::ast::can_have_modifiers` |
| `ast` | func | `FindAncestor` | 19 (flamework,transformer) | `find_ancestor` | `rotor_tsrs::ast::find_ancestor` |
| `ast` | func | `FindAncestorKind` | 1 (transformer) | `find_ancestor_kind` | `rotor_tsrs::ast::find_ancestor_kind` |
| `ast` | func | `GetClassExtendsHeritageElement` | 2 (transformer) | `get_class_extends_heritage_element` | `rotor_tsrs::ast::get_class_extends_heritage_element` |
| `ast` | func | `GetCombinedModifierFlags` | 1 (transformer) | `get_combined_modifier_flags` | `rotor_tsrs::ast::get_combined_modifier_flags` |
| `ast` | func | `GetExtendsHeritageClauseElement` | 2 (flamework,transformer) | `get_class_extends_heritage_element` | `rotor_tsrs::ast::get_class_extends_heritage_element` |
| `ast` | func | `GetFirstIdentifier` | 2 (transformer) | `get_first_identifier` | `rotor_tsrs::ast::get_first_identifier` |
| `ast` | func | `GetFunctionFlags` | 1 (transformer) | `get_function_flags` | `rotor_tsrs::ast::get_function_flags` |
| `ast` | func | `GetHeritageElements` | 1 (transformer) | `get_heritage_elements` | `rotor_tsrs::ast::get_heritage_elements` |
| `ast` | func | `GetImplementsHeritageClauseElements` | 2 (flamework) | `get_implements_heritage_clause_elements` | `rotor_tsrs::ast::get_implements_heritage_clause_elements` |
| `ast` | func | `GetImplementsTypeNodes` | 4 (flamework,transformer) | get_implements_heritage_clause_elements (TypeReference nodes in 7.1) | `rotor_tsrs::ast::get_implements_heritage_clause_elements` |
| `ast` | func | `GetNameOfDeclaration` | 4 (flamework) | `get_name_of_declaration` | `rotor_tsrs::ast::get_name_of_declaration` |
| `ast` | func | `GetPragmaFromSourceFile` | 1 (compile) | `get_pragma_from_source_file` | `rotor_tsrs::ast::get_pragma_from_source_file` |
| `ast` | func | `GetPropertyNameForPropertyNameNode` | 6 (flamework,transformer) | `get_property_name_for_property_name_node` | `rotor_tsrs::ast::get_property_name_for_property_name_node` |
| `ast` | func | `GetSourceFileOfNode` | 61 (compile,flamework,jecs,transformer) | `get_source_file_of_node` | `rotor_tsrs::ast::get_source_file_of_node` |
| `ast` | func | `GetThisContainer` | 1 (transformer) | `get_this_container` | `rotor_tsrs::ast::get_this_container` |
| `ast` | func | `HasAbstractModifier` | 2 (transformer) | `has_abstract_modifier` | `rotor_tsrs::ast::has_abstract_modifier` |
| `ast` | func | `HasDecorators` | 2 (transformer) | `has_decorators` | `rotor_tsrs::ast::has_decorators` |
| `ast` | func | `HasStaticModifier` | 10 (flamework,transformer) | `has_static_modifier` | `rotor_tsrs::ast::has_static_modifier` |
| `ast` | func | `HasSyntacticModifier` | 21 (transformer) | `has_syntactic_modifier` | `rotor_tsrs::ast::has_syntactic_modifier` |
| `ast` | func | `IsAccessor` | 2 (transformer) | `is_accessor` | `rotor_tsrs::ast::is_accessor` |
| `ast` | func | `IsAnyImportSyntax` | 1 (transformer) | `is_any_import_syntax` | `rotor_tsrs::ast::is_any_import_syntax` |
| `ast` | func | `IsArrayBindingPattern` | 19 (jecs,transformer) | `is_array_binding_pattern` | `rotor_tsrs::ast::is_array_binding_pattern` |
| `ast` | func | `IsArrayLiteralExpression` | 19 (transformer) | `is_array_literal_expression` | `rotor_tsrs::ast::is_array_literal_expression` |
| `ast` | func | `IsAsExpression` | 2 (flamework) | is_as_expression is pub(crate); kind test | `rotor_tsrs::ast::is_as_expression` |
| `ast` | func | `IsAssignmentExpression` | 1 (transformer) | `is_assignment_expression` | `rotor_tsrs::ast::is_assignment_expression` |
| `ast` | func | `IsAssignmentOperator` | 2 (transformer) | `is_assignment_operator` | `rotor_tsrs::ast::is_assignment_operator` |
| `ast` | func | `IsAssignmentTarget` | 1 (transformer) | `is_assignment_target` | `rotor_tsrs::ast::is_assignment_target` |
| `ast` | func | `IsAutoAccessorPropertyDeclaration` | 1 (transformer) | `is_auto_accessor_property_declaration` | `rotor_tsrs::ast::is_auto_accessor_property_declaration` |
| `ast` | func | `IsBinaryExpression` | 11 (transformer) | `is_binary_expression` | `rotor_tsrs::ast::is_binary_expression` |
| `ast` | func | `IsBindingElement` | 5 (jecs,transformer) | `is_binding_element` | `rotor_tsrs::ast::is_binding_element` |
| `ast` | func | `IsBlock` | 7 (flamework,transformer) | `is_block` | `rotor_tsrs::ast::is_block` |
| `ast` | func | `IsBreakStatement` | 1 (jecs) | none; kind test | `rotor_tsrs::ast::is_break_statement` |
| `ast` | func | `IsCallExpression` | 35 (flamework,jecs,transformer) | `is_call_expression` | `rotor_tsrs::ast::is_call_expression` |
| `ast` | func | `IsCaseClause` | 3 (transformer) | `is_case_clause` | `rotor_tsrs::ast::is_case_clause` |
| `ast` | func | `IsClassDeclaration` | 11 (flamework,transformer) | `is_class_declaration` | `rotor_tsrs::ast::is_class_declaration` |
| `ast` | func | `IsClassElement` | 2 (flamework,transformer) | `is_class_element` | `rotor_tsrs::ast::is_class_element` |
| `ast` | func | `IsClassExpression` | 5 (flamework,transformer) | `is_class_expression` | `rotor_tsrs::ast::is_class_expression` |
| `ast` | func | `IsClassLike` | 15 (flamework,transformer) | `is_class_like` | `rotor_tsrs::ast::is_class_like` |
| `ast` | func | `IsClassStaticBlockDeclaration` | 3 (transformer) | `is_class_static_block_declaration` | `rotor_tsrs::ast::is_class_static_block_declaration` |
| `ast` | func | `IsComputedPropertyName` | 3 (transformer) | `is_computed_property_name` | `rotor_tsrs::ast::is_computed_property_name` |
| `ast` | func | `IsConditionalTypeNode` | 1 (flamework) | `is_conditional_type_node` | `rotor_tsrs::ast::is_conditional_type_node` |
| `ast` | func | `IsConstructSignatureDeclaration` | 2 (transformer) | `is_construct_signature_declaration` | `rotor_tsrs::ast::is_construct_signature_declaration` |
| `ast` | func | `IsConstructorDeclaration` | 7 (flamework,transformer) | `is_constructor_declaration` | `rotor_tsrs::ast::is_constructor_declaration` |
| `ast` | func | `IsDeclaration` | 2 (flamework) | `is_declaration` | `rotor_tsrs::ast::is_declaration` |
| `ast` | func | `IsDecorator` | 4 (flamework) | `is_decorator` | `rotor_tsrs::ast::is_decorator` |
| `ast` | func | `IsDefaultClause` | 5 (transformer) | `is_default_clause` | `rotor_tsrs::ast::is_default_clause` |
| `ast` | func | `IsDeleteExpression` | 3 (transformer) | none; kind test | `rotor_tsrs::ast::is_delete_expression` |
| `ast` | func | `IsElementAccessExpression` | 19 (flamework,transformer) | `is_element_access_expression` | `rotor_tsrs::ast::is_element_access_expression` |
| `ast` | func | `IsEmptyStatement` | 1 (transformer) | `is_empty_statement` | `rotor_tsrs::ast::is_empty_statement` |
| `ast` | func | `IsEnumDeclaration` | 4 (flamework,transformer) | `is_enum_declaration` | `rotor_tsrs::ast::is_enum_declaration` |
| `ast` | func | `IsExportAssignment` | 2 (transformer) | `is_export_assignment` | `rotor_tsrs::ast::is_export_assignment` |
| `ast` | func | `IsExportDeclaration` | 3 (transformer) | `is_export_declaration` | `rotor_tsrs::ast::is_export_declaration` |
| `ast` | func | `IsExportSpecifier` | 4 (transformer) | `is_export_specifier` | `rotor_tsrs::ast::is_export_specifier` |
| `ast` | func | `IsExpression` | 2 (flamework,jecs) | `is_expression` | `rotor_tsrs::ast::is_expression` |
| `ast` | func | `IsExpressionStatement` | 5 (flamework,transformer) | `is_expression_statement` | `rotor_tsrs::ast::is_expression_statement` |
| `ast` | func | `IsExternalModule` | 3 (compile,flamework) | `is_external_module` | `rotor_tsrs::ast::is_external_module` |
| `ast` | func | `IsExternalModuleReference` | 1 (transformer) | `is_external_module_reference` | `rotor_tsrs::ast::is_external_module_reference` |
| `ast` | func | `IsForInStatement` | 1 (transformer) | `is_for_in_statement` | `rotor_tsrs::ast::is_for_in_statement` |
| `ast` | func | `IsForOfStatement` | 6 (jecs,transformer) | `is_for_of_statement` | `rotor_tsrs::ast::is_for_of_statement` |
| `ast` | func | `IsForStatement` | 3 (transformer) | `is_for_statement` | `rotor_tsrs::ast::is_for_statement` |
| `ast` | func | `IsFunctionDeclaration` | 6 (transformer) | `is_function_declaration` | `rotor_tsrs::ast::is_function_declaration` |
| `ast` | func | `IsFunctionExpression` | 8 (transformer) | `is_function_expression` | `rotor_tsrs::ast::is_function_expression` |
| `ast` | func | `IsFunctionLike` | 11 (transformer) | `is_function_like` | `rotor_tsrs::ast::is_function_like` |
| `ast` | func | `IsFunctionLikeDeclaration` | 3 (transformer) | `is_function_like_declaration` | `rotor_tsrs::ast::is_function_like_declaration` |
| `ast` | func | `IsGetAccessorDeclaration` | 1 (flamework) | `is_get_accessor_declaration` | `rotor_tsrs::ast::is_get_accessor_declaration` |
| `ast` | func | `IsIdentifier` | 80 (compile,flamework,jecs,transformer) | `is_identifier` | `rotor_tsrs::ast::is_identifier` |
| `ast` | func | `IsIfStatement` | 3 (jecs,transformer) | `is_if_statement` | `rotor_tsrs::ast::is_if_statement` |
| `ast` | func | `IsImportCall` | 2 (transformer) | `is_import_call` | `rotor_tsrs::ast::is_import_call` |
| `ast` | func | `IsImportDeclaration` | 5 (flamework) | `is_import_declaration` | `rotor_tsrs::ast::is_import_declaration` |
| `ast` | func | `IsIndexSignatureDeclaration` | 1 (transformer) | `is_index_signature_declaration` | `rotor_tsrs::ast::is_index_signature_declaration` |
| `ast` | func | `IsInstantiatedModule` | 2 (transformer) | `is_instantiated_module` | `rotor_tsrs::ast::is_instantiated_module` |
| `ast` | func | `IsInterfaceDeclaration` | 9 (transformer) | `is_interface_declaration` | `rotor_tsrs::ast::is_interface_declaration` |
| `ast` | func | `IsIterationStatement` | 7 (jecs,transformer) | `is_iteration_statement` | `rotor_tsrs::ast::is_iteration_statement` |
| `ast` | func | `IsJSDocLink` | 1 (flamework) | none; kind test | `rotor_tsrs::ast::is_jsdoc_link` |
| `ast` | func | `IsJSDocLinkCode` | 1 (flamework) | none; kind test | `rotor_tsrs::ast::is_jsdoc_link_code` |
| `ast` | func | `IsJSDocLinkLike` | 1 (flamework) | none; kind test | `rotor_tsrs::ast::is_jsdoc_link_like` |
| `ast` | func | `IsJSDocLinkPlain` | 1 (flamework) | none; kind test | `rotor_tsrs::ast::is_jsdoc_link_plain` |
| `ast` | func | `IsJsonSourceFile` | 1 (transformer) | `is_json_source_file` | `rotor_tsrs::ast::is_json_source_file` |
| `ast` | func | `IsJsxAttribute` | 1 (transformer) | `is_jsx_attribute` | `rotor_tsrs::ast::is_jsx_attribute` |
| `ast` | func | `IsJsxElement` | 1 (transformer) | `is_jsx_element` | `rotor_tsrs::ast::is_jsx_element` |
| `ast` | func | `IsJsxExpression` | 3 (transformer) | `is_jsx_expression` | `rotor_tsrs::ast::is_jsx_expression` |
| `ast` | func | `IsJsxFragment` | 1 (transformer) | `is_jsx_fragment` | `rotor_tsrs::ast::is_jsx_fragment` |
| `ast` | func | `IsJsxNamespacedName` | 1 (transformer) | `is_jsx_namespaced_name` | `rotor_tsrs::ast::is_jsx_namespaced_name` |
| `ast` | func | `IsJsxSelfClosingElement` | 1 (transformer) | `is_jsx_self_closing_element` | `rotor_tsrs::ast::is_jsx_self_closing_element` |
| `ast` | func | `IsJsxText` | 2 (transformer) | `is_jsx_text` | `rotor_tsrs::ast::is_jsx_text` |
| `ast` | func | `IsLabeledStatement` | 3 (transformer) | `is_labeled_statement` | `rotor_tsrs::ast::is_labeled_statement` |
| `ast` | func | `IsLiteralExpression` | 1 (flamework) | `is_literal_expression` | `rotor_tsrs::ast::is_literal_expression` |
| `ast` | func | `IsLogicalOrCoalescingAssignmentExpression` | 2 (transformer) | `is_logical_or_coalescing_assignment_expression` | `rotor_tsrs::ast::is_logical_or_coalescing_assignment_expression` |
| `ast` | func | `IsMethodDeclaration` | 11 (flamework,transformer) | `is_method_declaration` | `rotor_tsrs::ast::is_method_declaration` |
| `ast` | func | `IsMethodSignatureDeclaration` | 2 (transformer) | `is_method_signature_declaration` | `rotor_tsrs::ast::is_method_signature_declaration` |
| `ast` | func | `IsModuleBlock` | 3 (transformer) | `is_module_block` | `rotor_tsrs::ast::is_module_block` |
| `ast` | func | `IsModuleDeclaration` | 8 (transformer) | `is_module_declaration` | `rotor_tsrs::ast::is_module_declaration` |
| `ast` | func | `IsNamedExports` | 3 (transformer) | `is_named_exports` | `rotor_tsrs::ast::is_named_exports` |
| `ast` | func | `IsNamedImports` | 5 (flamework,transformer) | `is_named_imports` | `rotor_tsrs::ast::is_named_imports` |
| `ast` | func | `IsNamespaceExport` | 1 (transformer) | `is_namespace_export` | `rotor_tsrs::ast::is_namespace_export` |
| `ast` | func | `IsNamespaceImport` | 3 (transformer) | `is_namespace_import` | `rotor_tsrs::ast::is_namespace_import` |
| `ast` | func | `IsNewExpression` | 5 (flamework,transformer) | `is_new_expression` | `rotor_tsrs::ast::is_new_expression` |
| `ast` | func | `IsNoSubstitutionTemplateLiteral` | 1 (transformer) | `is_no_substitution_template_literal` | `rotor_tsrs::ast::is_no_substitution_template_literal` |
| `ast` | func | `IsNumericLiteral` | 1 (transformer) | `is_numeric_literal` | `rotor_tsrs::ast::is_numeric_literal` |
| `ast` | func | `IsObjectBindingPattern` | 1 (transformer) | `is_object_binding_pattern` | `rotor_tsrs::ast::is_object_binding_pattern` |
| `ast` | func | `IsObjectLiteralElement` | 1 (transformer) | `is_object_literal_element` | `rotor_tsrs::ast::is_object_literal_element` |
| `ast` | func | `IsObjectLiteralExpression` | 20 (flamework,jecs,transformer) | `is_object_literal_expression` | `rotor_tsrs::ast::is_object_literal_expression` |
| `ast` | func | `IsOmittedExpression` | 6 (jecs,transformer) | `is_omitted_expression` | `rotor_tsrs::ast::is_omitted_expression` |
| `ast` | func | `IsParameterDeclaration` | 3 (jecs,transformer) | `is_parameter_declaration` | `rotor_tsrs::ast::is_parameter_declaration` |
| `ast` | func | `IsParameterPropertyDeclaration` | 1 (transformer) | `is_parameter_property_declaration` | `rotor_tsrs::ast::is_parameter_property_declaration` |
| `ast` | func | `IsPartOfTypeNode` | 1 (transformer) | `is_part_of_type_node` | `rotor_tsrs::ast::is_part_of_type_node` |
| `ast` | func | `IsPartOfTypeQuery` | 1 (transformer) | `is_part_of_type_query` | `rotor_tsrs::ast::is_part_of_type_query` |
| `ast` | func | `IsPostfixUnaryExpression` | 4 (flamework,transformer) | `is_postfix_unary_expression` | `rotor_tsrs::ast::is_postfix_unary_expression` |
| `ast` | func | `IsPrefixUnaryExpression` | 6 (transformer) | `is_prefix_unary_expression` | `rotor_tsrs::ast::is_prefix_unary_expression` |
| `ast` | func | `IsPrivateIdentifier` | 5 (jecs,transformer) | `is_private_identifier` | `rotor_tsrs::ast::is_private_identifier` |
| `ast` | func | `IsPropertyAccessExpression` | 31 (flamework,jecs,transformer) | `is_property_access_expression` | `rotor_tsrs::ast::is_property_access_expression` |
| `ast` | func | `IsPropertyAssignment` | 9 (flamework,jecs,transformer) | `is_property_assignment` | `rotor_tsrs::ast::is_property_assignment` |
| `ast` | func | `IsPropertyDeclaration` | 8 (flamework,transformer) | `is_property_declaration` | `rotor_tsrs::ast::is_property_declaration` |
| `ast` | func | `IsPrototypeAccess` | 2 (transformer) | `is_prototype_access` | `rotor_tsrs::ast::is_prototype_access` |
| `ast` | func | `IsQualifiedName` | 1 (transformer) | `is_qualified_name` | `rotor_tsrs::ast::is_qualified_name` |
| `ast` | func | `IsReturnStatement` | 2 (transformer) | `is_return_statement` | `rotor_tsrs::ast::is_return_statement` |
| `ast` | func | `IsSemicolonClassElement` | 1 (transformer) | none; kind test | `rotor_tsrs::ast::is_semicolon_class_element` |
| `ast` | func | `IsSetAccessorDeclaration` | 1 (flamework) | `is_set_accessor_declaration` | `rotor_tsrs::ast::is_set_accessor_declaration` |
| `ast` | func | `IsShorthandPropertyAssignment` | 6 (transformer) | `is_shorthand_property_assignment` | `rotor_tsrs::ast::is_shorthand_property_assignment` |
| `ast` | func | `IsSourceFile` | 10 (flamework,transformer) | `is_source_file` | `rotor_tsrs::ast::is_source_file` |
| `ast` | func | `IsSpreadAssignment` | 3 (flamework,transformer) | `is_spread_assignment` | `rotor_tsrs::ast::is_spread_assignment` |
| `ast` | func | `IsSpreadElement` | 13 (flamework,transformer) | `is_spread_element` | `rotor_tsrs::ast::is_spread_element` |
| `ast` | func | `IsStatement` | 13 (flamework,jecs,transformer) | `is_statement` | `rotor_tsrs::ast::is_statement` |
| `ast` | func | `IsStringLiteral` | 20 (compile,flamework,transformer) | `is_string_literal` | `rotor_tsrs::ast::is_string_literal` |
| `ast` | func | `IsStringLiteralLike` | 7 (compile,transformer) | `is_string_literal_like` | `rotor_tsrs::ast::is_string_literal_like` |
| `ast` | func | `IsSuperCall` | 3 (flamework,transformer) | tsrs_checker::is_super_call is pub(crate) | `rotor_tsrs::ast::is_super_call` |
| `ast` | func | `IsSuperProperty` | 3 (transformer) | `is_super_property` | `rotor_tsrs::ast::is_super_property` |
| `ast` | func | `IsSwitchStatement` | 3 (jecs,transformer) | `is_switch_statement` | `rotor_tsrs::ast::is_switch_statement` |
| `ast` | func | `IsTemplateExpression` | 1 (transformer) | `is_template_expression` | `rotor_tsrs::ast::is_template_expression` |
| `ast` | func | `IsThisIdentifier` | 4 (transformer) | `is_this_identifier` | `rotor_tsrs::ast::is_this_identifier` |
| `ast` | func | `IsTryStatement` | 6 (transformer) | `is_try_statement` | `rotor_tsrs::ast::is_try_statement` |
| `ast` | func | `IsTypeAliasDeclaration` | 9 (flamework,transformer) | `is_type_alias_declaration` | `rotor_tsrs::ast::is_type_alias_declaration` |
| `ast` | func | `IsTypeParameterDeclaration` | 2 (flamework,transformer) | `is_type_parameter_declaration` | `rotor_tsrs::ast::is_type_parameter_declaration` |
| `ast` | func | `IsTypeQueryNode` | 1 (flamework) | `is_type_query_node` | `rotor_tsrs::ast::is_type_query_node` |
| `ast` | func | `IsTypeReferenceNode` | 2 (flamework) | `is_type_reference_node` | `rotor_tsrs::ast::is_type_reference_node` |
| `ast` | func | `IsVarConst` | 3 (transformer) | `is_var_const` | `rotor_tsrs::ast::is_var_const` |
| `ast` | func | `IsVariableDeclaration` | 11 (jecs,transformer) | `is_variable_declaration` | `rotor_tsrs::ast::is_variable_declaration` |
| `ast` | func | `IsVariableDeclarationList` | 14 (jecs,transformer) | `is_variable_declaration_list` | `rotor_tsrs::ast::is_variable_declaration_list` |
| `ast` | func | `IsVariableStatement` | 2 (transformer) | `is_variable_statement` | `rotor_tsrs::ast::is_variable_statement` |
| `ast` | func | `IsVoidExpression` | 1 (transformer) | `is_void_expression` | `rotor_tsrs::ast::is_void_expression` |
| `ast` | func | `IsWriteAccess` | 2 (transformer) | `is_write_access` | `rotor_tsrs::ast::is_write_access` |
| `ast` | func | `NewCompilerDiagnostic` | 2 (compile) | `new_compiler_diagnostic` | `rotor_tsrs::ast::new_compiler_diagnostic` |
| `ast` | func | `NewDiagnostic` | 3 (flamework) | `new_diagnostic` | `rotor_tsrs::ast::new_diagnostic` |
| `ast` | func | `NewDiagnosticWithStringCode` | 5 (flamework) | — | missing: rotor's own tsgo patch for rbxts string codes; tsrs diagnostics carry numeric codes, so Stage 5 keeps the string code beside the tsrs diagnostic |
| `ast` | func | `NewNodeVisitor` | 5 (flamework) | `new_node_visitor` | `rotor_tsrs::ast::new_node_visitor` |
| `ast` | func | `PositionIsSynthesized` | 1 (transformer) | `position_is_synthesized` | `rotor_tsrs::ast::position_is_synthesized` |
| `ast` | func | `SetParentInChildren` | 3 (flamework) | `set_parent_in_children` | `rotor_tsrs::ast::set_parent_in_children` |
| `ast` | func | `SkipParentheses` | 3 (flamework) | `skip_parentheses` | `rotor_tsrs::ast::skip_parentheses` |
| `ast` | func | `TryGetTextOfPropertyName` | 1 (transformer) | `try_get_text_of_property_name` | `rotor_tsrs::ast::try_get_text_of_property_name` |
| `ast` | method | `BindingElement.Name` | 9 (transformer) | `BindingElement::name` | `rotor_tsrs::ast::BindingElement::name` |
| `ast` | method | `ClassDeclaration.Name` | 1 (flamework) | `ClassDeclaration::name` | `rotor_tsrs::ast::ClassDeclaration::name` |
| `ast` | method | `Diagnostic.AddRelatedInfo` | 3 (flamework) | `DiagnosticExt::add_related_info` | `rotor_tsrs::ast::DiagnosticExt::add_related_info` |
| `ast` | method | `Diagnostic.Category` | 1 (compile) | `Diagnostic::category` | `rotor_tsrs::ast::Diagnostic::category` |
| `ast` | method | `Diagnostic.Code` | 5 (compile,flamework) | `Diagnostic::code` | `rotor_tsrs::ast::Diagnostic::code` |
| `ast` | method | `Diagnostic.File` | 3 (compile,flamework) | `Diagnostic::file` | `rotor_tsrs::ast::Diagnostic::file` |
| `ast` | method | `Diagnostic.Len` | 1 (compile) | `Diagnostic::len` | `rotor_tsrs::ast::Diagnostic::len` |
| `ast` | method | `Diagnostic.Pos` | 10 (compile,flamework) | `Diagnostic::pos` | `rotor_tsrs::ast::Diagnostic::pos` |
| `ast` | method | `Diagnostic.String` | 8 (compile,flamework) | `Diagnostic::localize` | `rotor_tsrs::ast::Diagnostic::localize` |
| `ast` | method | `Diagnostic.StringCode` | 1 (compile) | — | missing: same rotor-only string code as NewDiagnosticWithStringCode |
| `ast` | method | `FunctionDeclaration.Name` | 3 (transformer) | `FunctionDeclaration::name` | `rotor_tsrs::ast::FunctionDeclaration::name` |
| `ast` | method | `FunctionExpression.Name` | 3 (transformer) | `FunctionExpression::name` | `rotor_tsrs::ast::FunctionExpression::name` |
| `ast` | method | `GetAccessorDeclaration.Name` | 1 (flamework) | `GetAccessorDeclaration::name` | `rotor_tsrs::ast::GetAccessorDeclaration::name` |
| `ast` | method | `ImportClause.Name` | 7 (flamework,transformer) | `ImportClause::name` | `rotor_tsrs::ast::ImportClause::name` |
| `ast` | method | `ImportEqualsDeclaration.Name` | 4 (transformer) | `ImportEqualsDeclaration::name` | `rotor_tsrs::ast::ImportEqualsDeclaration::name` |
| `ast` | method | `ImportSpecifier.Name` | 9 (flamework) | `ImportSpecifier::name` | `rotor_tsrs::ast::ImportSpecifier::name` |
| `ast` | method | `JsxAttribute.Name` | 1 (transformer) | `JsxAttribute::name` | `rotor_tsrs::ast::JsxAttribute::name` |
| `ast` | method | `JsxNamespacedName.Name` | 1 (transformer) | `JsxNamespacedName::name` | `rotor_tsrs::ast::JsxNamespacedName::name` |
| `ast` | method | `Kind.String` | 5 (flamework,jecs,transformer) | Debug for Kind (no Kind prefix) | `rotor_tsrs::ast::kind_string` |
| `ast` | method | `MethodDeclaration.Name` | 2 (flamework,transformer) | `MethodDeclaration::name` | `rotor_tsrs::ast::MethodDeclaration::name` |
| `ast` | method | `ModifiersBase.Modifiers` | 2 (flamework) | `ModifiersBase::modifiers` | `rotor_tsrs::ast::ModifiersBase::modifiers` |
| `ast` | method | `NamedMemberBase.Modifiers` | 2 (flamework) | `NamedMemberBase::modifiers` | `rotor_tsrs::ast::NamedMemberBase::modifiers` |
| `ast` | method | `Node.Arguments` | 29 (compile,flamework,jecs,transformer) | `Node::arguments` | `rotor_tsrs::ast::Node::arguments` |
| `ast` | method | `Node.AsArrayLiteralExpression` | 10 (flamework,transformer) | `Node::as_array_literal_expression` | `rotor_tsrs::ast::Node::as_array_literal_expression` |
| `ast` | method | `Node.AsArrayTypeNode` | 3 (transformer) | `Node::as_array_type_node` | `rotor_tsrs::ast::Node::as_array_type_node` |
| `ast` | method | `Node.AsAwaitExpression` | 1 (transformer) | `Node::as_await_expression` | `rotor_tsrs::ast::Node::as_await_expression` |
| `ast` | method | `Node.AsBinaryExpression` | 21 (flamework,transformer) | `Node::as_binary_expression` | `rotor_tsrs::ast::Node::as_binary_expression` |
| `ast` | method | `Node.AsBindingElement` | 10 (transformer) | `Node::as_binding_element` | `rotor_tsrs::ast::Node::as_binding_element` |
| `ast` | method | `Node.AsBindingPattern` | 14 (jecs,transformer) | `Node::as_binding_pattern` | `rotor_tsrs::ast::Node::as_binding_pattern` |
| `ast` | method | `Node.AsBlock` | 11 (transformer) | `Node::as_block` | `rotor_tsrs::ast::Node::as_block` |
| `ast` | method | `Node.AsBreakStatement` | 2 (transformer) | `Node::as_break_statement` | `rotor_tsrs::ast::Node::as_break_statement` |
| `ast` | method | `Node.AsCallExpression` | 27 (compile,flamework,jecs,transformer) | `Node::as_call_expression` | `rotor_tsrs::ast::Node::as_call_expression` |
| `ast` | method | `Node.AsCaseBlock` | 3 (transformer) | `Node::as_case_block` | `rotor_tsrs::ast::Node::as_case_block` |
| `ast` | method | `Node.AsCaseOrDefaultClause` | 6 (transformer) | `Node::as_case_or_default_clause` | `rotor_tsrs::ast::Node::as_case_or_default_clause` |
| `ast` | method | `Node.AsCatchClause` | 2 (transformer) | `Node::as_catch_clause` | `rotor_tsrs::ast::Node::as_catch_clause` |
| `ast` | method | `Node.AsClassDeclaration` | 5 (flamework) | `Node::as_class_declaration` | `rotor_tsrs::ast::Node::as_class_declaration` |
| `ast` | method | `Node.AsClassStaticBlockDeclaration` | 1 (transformer) | `Node::as_class_static_block_declaration` | `rotor_tsrs::ast::Node::as_class_static_block_declaration` |
| `ast` | method | `Node.AsComputedPropertyName` | 3 (transformer) | `Node::as_computed_property_name` | `rotor_tsrs::ast::Node::as_computed_property_name` |
| `ast` | method | `Node.AsConditionalExpression` | 1 (transformer) | `Node::as_conditional_expression` | `rotor_tsrs::ast::Node::as_conditional_expression` |
| `ast` | method | `Node.AsConditionalTypeNode` | 2 (flamework,transformer) | `Node::as_conditional_type_node` | `rotor_tsrs::ast::Node::as_conditional_type_node` |
| `ast` | method | `Node.AsContinueStatement` | 2 (transformer) | `Node::as_continue_statement` | `rotor_tsrs::ast::Node::as_continue_statement` |
| `ast` | method | `Node.AsDecorator` | 3 (flamework) | `Node::as_decorator` | `rotor_tsrs::ast::Node::as_decorator` |
| `ast` | method | `Node.AsDeleteExpression` | 2 (flamework,transformer) | `Node::as_delete_expression` | `rotor_tsrs::ast::Node::as_delete_expression` |
| `ast` | method | `Node.AsDoStatement` | 1 (transformer) | `Node::as_do_statement` | `rotor_tsrs::ast::Node::as_do_statement` |
| `ast` | method | `Node.AsElementAccessExpression` | 16 (compile,flamework,transformer) | `Node::as_element_access_expression` | `rotor_tsrs::ast::Node::as_element_access_expression` |
| `ast` | method | `Node.AsEnumDeclaration` | 4 (flamework,transformer) | `Node::as_enum_declaration` | `rotor_tsrs::ast::Node::as_enum_declaration` |
| `ast` | method | `Node.AsEnumMember` | 1 (transformer) | `Node::as_enum_member` | `rotor_tsrs::ast::Node::as_enum_member` |
| `ast` | method | `Node.AsExportAssignment` | 6 (transformer) | `Node::as_export_assignment` | `rotor_tsrs::ast::Node::as_export_assignment` |
| `ast` | method | `Node.AsExportDeclaration` | 6 (compile,transformer) | `Node::as_export_declaration` | `rotor_tsrs::ast::Node::as_export_declaration` |
| `ast` | method | `Node.AsExportSpecifier` | 2 (transformer) | `Node::as_export_specifier` | `rotor_tsrs::ast::Node::as_export_specifier` |
| `ast` | method | `Node.AsExpressionStatement` | 1 (transformer) | `Node::as_expression_statement` | `rotor_tsrs::ast::Node::as_expression_statement` |
| `ast` | method | `Node.AsExpressionWithTypeArguments` | 2 (flamework) | `Node::as_expression_with_type_arguments` | `rotor_tsrs::ast::Node::as_expression_with_type_arguments` |
| `ast` | method | `Node.AsExternalModuleReference` | 2 (compile,transformer) | `Node::as_external_module_reference` | `rotor_tsrs::ast::Node::as_external_module_reference` |
| `ast` | method | `Node.AsForInOrOfStatement` | 7 (jecs,transformer) | `Node::as_for_in_or_of_statement` | `rotor_tsrs::ast::Node::as_for_in_or_of_statement` |
| `ast` | method | `Node.AsForStatement` | 5 (transformer) | `Node::as_for_statement` | `rotor_tsrs::ast::Node::as_for_statement` |
| `ast` | method | `Node.AsFunctionDeclaration` | 3 (transformer) | `Node::as_function_declaration` | `rotor_tsrs::ast::Node::as_function_declaration` |
| `ast` | method | `Node.AsFunctionExpression` | 6 (transformer) | `Node::as_function_expression` | `rotor_tsrs::ast::Node::as_function_expression` |
| `ast` | method | `Node.AsFunctionTypeNode` | 2 (transformer) | `Node::as_function_type_node` | `rotor_tsrs::ast::Node::as_function_type_node` |
| `ast` | method | `Node.AsGetAccessorDeclaration` | 1 (flamework) | `Node::as_get_accessor_declaration` | `rotor_tsrs::ast::Node::as_get_accessor_declaration` |
| `ast` | method | `Node.AsHeritageClause` | 1 (flamework) | `Node::as_heritage_clause` | `rotor_tsrs::ast::Node::as_heritage_clause` |
| `ast` | method | `Node.AsIfStatement` | 2 (transformer) | `Node::as_if_statement` | `rotor_tsrs::ast::Node::as_if_statement` |
| `ast` | method | `Node.AsImportClause` | 10 (flamework,transformer) | `Node::as_import_clause` | `rotor_tsrs::ast::Node::as_import_clause` |
| `ast` | method | `Node.AsImportDeclaration` | 11 (compile,flamework,transformer) | `Node::as_import_declaration` | `rotor_tsrs::ast::Node::as_import_declaration` |
| `ast` | method | `Node.AsImportEqualsDeclaration` | 1 (transformer) | `Node::as_import_equals_declaration` | `rotor_tsrs::ast::Node::as_import_equals_declaration` |
| `ast` | method | `Node.AsImportSpecifier` | 5 (flamework,transformer) | `Node::as_import_specifier` | `rotor_tsrs::ast::Node::as_import_specifier` |
| `ast` | method | `Node.AsImportTypeNode` | 1 (compile) | `Node::as_import_type_node` | `rotor_tsrs::ast::Node::as_import_type_node` |
| `ast` | method | `Node.AsIndexSignatureDeclaration` | 1 (transformer) | `Node::as_index_signature_declaration` | `rotor_tsrs::ast::Node::as_index_signature_declaration` |
| `ast` | method | `Node.AsIndexedAccessTypeNode` | 1 (transformer) | `Node::as_indexed_access_type_node` | `rotor_tsrs::ast::Node::as_indexed_access_type_node` |
| `ast` | method | `Node.AsInferTypeNode` | 2 (transformer) | `Node::as_infer_type_node` | `rotor_tsrs::ast::Node::as_infer_type_node` |
| `ast` | method | `Node.AsInterfaceDeclaration` | 3 (transformer) | `Node::as_interface_declaration` | `rotor_tsrs::ast::Node::as_interface_declaration` |
| `ast` | method | `Node.AsIntersectionTypeNode` | 1 (transformer) | `Node::as_intersection_type_node` | `rotor_tsrs::ast::Node::as_intersection_type_node` |
| `ast` | method | `Node.AsJSDoc` | 6 (flamework) | `Node::as_jsdoc` | `rotor_tsrs::ast::Node::as_jsdoc` |
| `ast` | method | `Node.AsJsxAttribute` | 1 (transformer) | `Node::as_jsx_attribute` | `rotor_tsrs::ast::Node::as_jsx_attribute` |
| `ast` | method | `Node.AsJsxAttributes` | 2 (transformer) | `Node::as_jsx_attributes` | `rotor_tsrs::ast::Node::as_jsx_attributes` |
| `ast` | method | `Node.AsJsxElement` | 1 (transformer) | `Node::as_jsx_element` | `rotor_tsrs::ast::Node::as_jsx_element` |
| `ast` | method | `Node.AsJsxExpression` | 4 (transformer) | `Node::as_jsx_expression` | `rotor_tsrs::ast::Node::as_jsx_expression` |
| `ast` | method | `Node.AsJsxFragment` | 1 (transformer) | `Node::as_jsx_fragment` | `rotor_tsrs::ast::Node::as_jsx_fragment` |
| `ast` | method | `Node.AsJsxNamespacedName` | 1 (transformer) | `Node::as_jsx_namespaced_name` | `rotor_tsrs::ast::Node::as_jsx_namespaced_name` |
| `ast` | method | `Node.AsJsxOpeningElement` | 1 (transformer) | `Node::as_jsx_opening_element` | `rotor_tsrs::ast::Node::as_jsx_opening_element` |
| `ast` | method | `Node.AsJsxSelfClosingElement` | 1 (transformer) | `Node::as_jsx_self_closing_element` | `rotor_tsrs::ast::Node::as_jsx_self_closing_element` |
| `ast` | method | `Node.AsJsxSpreadAttribute` | 1 (transformer) | `Node::as_jsx_spread_attribute` | `rotor_tsrs::ast::Node::as_jsx_spread_attribute` |
| `ast` | method | `Node.AsJsxText` | 2 (transformer) | `Node::as_jsx_text` | `rotor_tsrs::ast::Node::as_jsx_text` |
| `ast` | method | `Node.AsLabeledStatement` | 2 (transformer) | `Node::as_labeled_statement` | `rotor_tsrs::ast::Node::as_labeled_statement` |
| `ast` | method | `Node.AsLiteralTypeNode` | 3 (compile,transformer) | `Node::as_literal_type_node` | `rotor_tsrs::ast::Node::as_literal_type_node` |
| `ast` | method | `Node.AsMappedTypeNode` | 1 (transformer) | `Node::as_mapped_type_node` | `rotor_tsrs::ast::Node::as_mapped_type_node` |
| `ast` | method | `Node.AsMethodDeclaration` | 3 (flamework,transformer) | `Node::as_method_declaration` | `rotor_tsrs::ast::Node::as_method_declaration` |
| `ast` | method | `Node.AsModuleBlock` | 1 (transformer) | `Node::as_module_block` | `rotor_tsrs::ast::Node::as_module_block` |
| `ast` | method | `Node.AsModuleDeclaration` | 1 (transformer) | `Node::as_module_declaration` | `rotor_tsrs::ast::Node::as_module_declaration` |
| `ast` | method | `Node.AsNamedExports` | 3 (transformer) | `Node::as_named_exports` | `rotor_tsrs::ast::Node::as_named_exports` |
| `ast` | method | `Node.AsNamedImports` | 8 (flamework,transformer) | `Node::as_named_imports` | `rotor_tsrs::ast::Node::as_named_imports` |
| `ast` | method | `Node.AsNamedTupleMember` | 1 (transformer) | `Node::as_named_tuple_member` | `rotor_tsrs::ast::Node::as_named_tuple_member` |
| `ast` | method | `Node.AsNewExpression` | 7 (flamework,transformer) | `Node::as_new_expression` | `rotor_tsrs::ast::Node::as_new_expression` |
| `ast` | method | `Node.AsNode` | 11 (flamework,transformer) | `Node::as_node` | `rotor_tsrs::ast::Node::as_node` |
| `ast` | method | `Node.AsObjectLiteralExpression` | 12 (flamework,jecs,transformer) | `Node::as_object_literal_expression` | `rotor_tsrs::ast::Node::as_object_literal_expression` |
| `ast` | method | `Node.AsParameterDeclaration` | 13 (flamework,transformer) | `Node::as_parameter_declaration` | `rotor_tsrs::ast::Node::as_parameter_declaration` |
| `ast` | method | `Node.AsParenthesizedExpression` | 1 (transformer) | `Node::as_parenthesized_expression` | `rotor_tsrs::ast::Node::as_parenthesized_expression` |
| `ast` | method | `Node.AsParenthesizedTypeNode` | 1 (transformer) | `Node::as_parenthesized_type_node` | `rotor_tsrs::ast::Node::as_parenthesized_type_node` |
| `ast` | method | `Node.AsPostfixUnaryExpression` | 5 (flamework,transformer) | `Node::as_postfix_unary_expression` | `rotor_tsrs::ast::Node::as_postfix_unary_expression` |
| `ast` | method | `Node.AsPrefixUnaryExpression` | 8 (flamework,transformer) | `Node::as_prefix_unary_expression` | `rotor_tsrs::ast::Node::as_prefix_unary_expression` |
| `ast` | method | `Node.AsPropertyAccessExpression` | 11 (jecs,transformer) | `Node::as_property_access_expression` | `rotor_tsrs::ast::Node::as_property_access_expression` |
| `ast` | method | `Node.AsPropertyAssignment` | 5 (flamework,transformer) | `Node::as_property_assignment` | `rotor_tsrs::ast::Node::as_property_assignment` |
| `ast` | method | `Node.AsPropertyDeclaration` | 2 (flamework) | `Node::as_property_declaration` | `rotor_tsrs::ast::Node::as_property_declaration` |
| `ast` | method | `Node.AsPropertySignatureDeclaration` | 1 (transformer) | `Node::as_property_signature_declaration` | `rotor_tsrs::ast::Node::as_property_signature_declaration` |
| `ast` | method | `Node.AsQualifiedName` | 2 (transformer) | `Node::as_qualified_name` | `rotor_tsrs::ast::Node::as_qualified_name` |
| `ast` | method | `Node.AsReturnStatement` | 1 (transformer) | `Node::as_return_statement` | `rotor_tsrs::ast::Node::as_return_statement` |
| `ast` | method | `Node.AsSetAccessorDeclaration` | 1 (flamework) | `Node::as_set_accessor_declaration` | `rotor_tsrs::ast::Node::as_set_accessor_declaration` |
| `ast` | method | `Node.AsShorthandPropertyAssignment` | 1 (transformer) | `Node::as_shorthand_property_assignment` | `rotor_tsrs::ast::Node::as_shorthand_property_assignment` |
| `ast` | method | `Node.AsSourceFile` | 8 (flamework,jecs,transformer) | `Node::as_source_file` | `rotor_tsrs::ast::Node::as_source_file` |
| `ast` | method | `Node.AsSpreadAssignment` | 4 (transformer) | `Node::as_spread_assignment` | `rotor_tsrs::ast::Node::as_spread_assignment` |
| `ast` | method | `Node.AsSpreadElement` | 4 (transformer) | `Node::as_spread_element` | `rotor_tsrs::ast::Node::as_spread_element` |
| `ast` | method | `Node.AsSwitchStatement` | 2 (transformer) | `Node::as_switch_statement` | `rotor_tsrs::ast::Node::as_switch_statement` |
| `ast` | method | `Node.AsTaggedTemplateExpression` | 1 (transformer) | `Node::as_tagged_template_expression` | `rotor_tsrs::ast::Node::as_tagged_template_expression` |
| `ast` | method | `Node.AsTemplateExpression` | 2 (transformer) | `Node::as_template_expression` | `rotor_tsrs::ast::Node::as_template_expression` |
| `ast` | method | `Node.AsTemplateLiteralTypeNode` | 1 (transformer) | `Node::as_template_literal_type_node` | `rotor_tsrs::ast::Node::as_template_literal_type_node` |
| `ast` | method | `Node.AsTemplateLiteralTypeSpan` | 1 (transformer) | `Node::as_template_literal_type_span` | `rotor_tsrs::ast::Node::as_template_literal_type_span` |
| `ast` | method | `Node.AsTemplateSpan` | 4 (transformer) | `Node::as_template_span` | `rotor_tsrs::ast::Node::as_template_span` |
| `ast` | method | `Node.AsThrowStatement` | 1 (transformer) | `Node::as_throw_statement` | `rotor_tsrs::ast::Node::as_throw_statement` |
| `ast` | method | `Node.AsTryStatement` | 2 (transformer) | `Node::as_try_statement` | `rotor_tsrs::ast::Node::as_try_statement` |
| `ast` | method | `Node.AsTupleTypeNode` | 1 (transformer) | `Node::as_tuple_type_node` | `rotor_tsrs::ast::Node::as_tuple_type_node` |
| `ast` | method | `Node.AsTypeAliasDeclaration` | 5 (flamework,transformer) | `Node::as_type_alias_declaration` | `rotor_tsrs::ast::Node::as_type_alias_declaration` |
| `ast` | method | `Node.AsTypeOperatorNode` | 7 (transformer) | `Node::as_type_operator_node` | `rotor_tsrs::ast::Node::as_type_operator_node` |
| `ast` | method | `Node.AsTypeParameterDeclaration` | 4 (flamework,transformer) | `Node::as_type_parameter_declaration` | `rotor_tsrs::ast::Node::as_type_parameter_declaration` |
| `ast` | method | `Node.AsTypeQueryNode` | 2 (flamework,transformer) | `Node::as_type_query_node` | `rotor_tsrs::ast::Node::as_type_query_node` |
| `ast` | method | `Node.AsTypeReferenceNode` | 5 (flamework,transformer) | `Node::as_type_reference_node` | `rotor_tsrs::ast::Node::as_type_reference_node` |
| `ast` | method | `Node.AsUnionTypeNode` | 1 (transformer) | `Node::as_union_type_node` | `rotor_tsrs::ast::Node::as_union_type_node` |
| `ast` | method | `Node.AsVariableDeclaration` | 9 (transformer) | `Node::as_variable_declaration` | `rotor_tsrs::ast::Node::as_variable_declaration` |
| `ast` | method | `Node.AsVariableDeclarationList` | 14 (jecs,transformer) | `Node::as_variable_declaration_list` | `rotor_tsrs::ast::Node::as_variable_declaration_list` |
| `ast` | method | `Node.AsVariableStatement` | 3 (transformer) | `Node::as_variable_statement` | `rotor_tsrs::ast::Node::as_variable_statement` |
| `ast` | method | `Node.AsVoidExpression` | 1 (transformer) | `Node::as_void_expression` | `rotor_tsrs::ast::Node::as_void_expression` |
| `ast` | method | `Node.AsWhileStatement` | 1 (transformer) | `Node::as_while_statement` | `rotor_tsrs::ast::Node::as_while_statement` |
| `ast` | method | `Node.AsYieldExpression` | 1 (transformer) | `Node::as_yield_expression` | `rotor_tsrs::ast::Node::as_yield_expression` |
| `ast` | method | `Node.Body` | 13 (transformer) | `Node::body` | `rotor_tsrs::ast::Node::body` |
| `ast` | method | `Node.Comments` | 3 (flamework) | `Node::comments` | `rotor_tsrs::ast::Node::comments` |
| `ast` | method | `Node.Decorators` | 5 (flamework) | `Node::decorators` | `rotor_tsrs::ast::Node::decorators` |
| `ast` | method | `Node.End` | 20 (compile,flamework,jecs,transformer) | `Node::end` | `rotor_tsrs::ast::Node::end` |
| `ast` | method | `Node.Expression` | 50 (compile,flamework,jecs,transformer) | `Node::expression` | `rotor_tsrs::ast::Node::expression` |
| `ast` | method | `Node.ForEachChild` | 30 (compile,flamework,jecs,transformer) | `Node::for_each_child` | `rotor_tsrs::ast::Node::for_each_child` |
| `ast` | method | `Node.FunctionLikeData` | 1 (transformer) | `Node::function_like_data` | `rotor_tsrs::ast::Node::function_like_data` |
| `ast` | method | `Node.Initializer` | 17 (flamework,jecs,transformer) | `Node::initializer` | `rotor_tsrs::ast::Node::initializer` |
| `ast` | method | `Node.IsTypeOnly` | 1 (transformer) | `Node::is_type_only` | `rotor_tsrs::ast::Node::is_type_only` |
| `ast` | method | `Node.IterChildren` | 4 (flamework) | `Node::iter_children` | `rotor_tsrs::ast::Node::iter_children` |
| `ast` | method | `Node.JSDoc` | 4 (flamework) | Node::jsdoc(file) | `rotor_tsrs::ast::Node::jsdoc` |
| `ast` | method | `Node.Members` | 10 (flamework,transformer) | `Node::members` | `rotor_tsrs::ast::Node::members` |
| `ast` | method | `Node.Modifiers` | 10 (flamework,transformer) | `Node::modifiers` | `rotor_tsrs::ast::Node::modifiers` |
| `ast` | method | `Node.ModuleSpecifier` | 1 (transformer) | `Node::module_specifier` | `rotor_tsrs::ast::Node::module_specifier` |
| `ast` | method | `Node.Name` | 158 (compile,flamework,jecs,transformer) | `Node::name` | `rotor_tsrs::ast::Node::name` |
| `ast` | method | `Node.Parameters` | 24 (flamework,transformer) | `Node::parameters` | `rotor_tsrs::ast::Node::parameters` |
| `ast` | method | `Node.Pos` | 24 (compile,flamework,jecs,transformer) | `Node::pos` | `rotor_tsrs::ast::Node::pos` |
| `ast` | method | `Node.PostfixToken` | 1 (flamework) | `Node::postfix_token` | `rotor_tsrs::ast::Node::postfix_token` |
| `ast` | method | `Node.PropertyName` | 1 (transformer) | `Node::property_name` | `rotor_tsrs::ast::Node::property_name` |
| `ast` | method | `Node.QuestionDotToken` | 4 (flamework,transformer) | `Node::question_dot_token` | `rotor_tsrs::ast::Node::question_dot_token` |
| `ast` | method | `Node.Statement` | 1 (transformer) | `Node::statement` | `rotor_tsrs::ast::Node::statement` |
| `ast` | method | `Node.Statements` | 1 (transformer) | `Node::statements` | `rotor_tsrs::ast::Node::statements` |
| `ast` | method | `Node.Symbol` | 16 (flamework,transformer) | `Node::symbol` | `rotor_tsrs::ast::Node::symbol` |
| `ast` | method | `Node.TagName` | 5 (flamework) | `Node::tag_name` | `rotor_tsrs::ast::Node::tag_name` |
| `ast` | method | `Node.Text` | 160 (compile,flamework,jecs,transformer) | `Node::text` | `rotor_tsrs::ast::Node::text` |
| `ast` | method | `Node.Type` | 6 (flamework) | `Node::type_` | `rotor_tsrs::ast::Node::type_` |
| `ast` | method | `Node.TypeArgumentList` | 1 (flamework) | `Node::type_argument_list` | `rotor_tsrs::ast::Node::type_argument_list` |
| `ast` | method | `Node.TypeArguments` | 4 (flamework,transformer) | `Node::type_arguments` | `rotor_tsrs::ast::Node::type_arguments` |
| `ast` | method | `Node.TypeParameters` | 6 (transformer) | `Node::type_parameters` | `rotor_tsrs::ast::Node::type_parameters` |
| `ast` | method | `NodeDefault.AsNode` | 26 (compile,flamework,jecs,transformer) | `NodeDefault::as_node` | `rotor_tsrs::ast::NodeDefault::as_node` |
| `ast` | method | `NodeFactory.AsNodeFactory` | 2 (flamework) | `tsrs_printer::NodeFactory::as_node_factory` | `rotor_tsrs::printer::NodeFactory::as_node_factory` |
| `ast` | method | `NodeFactory.DeepCloneNode` | 2 (flamework) | `NodeFactory::deep_clone_node` | `rotor_tsrs::ast::NodeFactory::deep_clone_node` |
| `ast` | method | `NodeFactory.DeepCloneReparse` | 2 (flamework) | `NodeFactory::deep_clone_reparse` | `rotor_tsrs::ast::NodeFactory::deep_clone_reparse` |
| `ast` | method | `NodeFactory.NewArrayLiteralExpression` | 10 (flamework) | `NodeFactory::new_array_literal_expression` | `rotor_tsrs::ast::NodeFactory::new_array_literal_expression` |
| `ast` | method | `NodeFactory.NewAsExpression` | 9 (flamework,jecs) | `NodeFactory::new_as_expression` | `rotor_tsrs::ast::NodeFactory::new_as_expression` |
| `ast` | method | `NodeFactory.NewBinaryExpression` | 4 (flamework,jecs) | `NodeFactory::new_binary_expression` | `rotor_tsrs::ast::NodeFactory::new_binary_expression` |
| `ast` | method | `NodeFactory.NewBlock` | 7 (flamework,jecs) | `NodeFactory::new_block` | `rotor_tsrs::ast::NodeFactory::new_block` |
| `ast` | method | `NodeFactory.NewBreakStatement` | 2 (jecs) | `NodeFactory::new_break_statement` | `rotor_tsrs::ast::NodeFactory::new_break_statement` |
| `ast` | method | `NodeFactory.NewCallExpression` | 9 (flamework,jecs) | `NodeFactory::new_call_expression` | `rotor_tsrs::ast::NodeFactory::new_call_expression` |
| `ast` | method | `NodeFactory.NewClassStaticBlockDeclaration` | 1 (flamework) | `NodeFactory::new_class_static_block_declaration` | `rotor_tsrs::ast::NodeFactory::new_class_static_block_declaration` |
| `ast` | method | `NodeFactory.NewComputedPropertyName` | 1 (flamework) | `NodeFactory::new_computed_property_name` | `rotor_tsrs::ast::NodeFactory::new_computed_property_name` |
| `ast` | method | `NodeFactory.NewConditionalExpression` | 1 (jecs) | `NodeFactory::new_conditional_expression` | `rotor_tsrs::ast::NodeFactory::new_conditional_expression` |
| `ast` | method | `NodeFactory.NewContinueStatement` | 1 (jecs) | `NodeFactory::new_continue_statement` | `rotor_tsrs::ast::NodeFactory::new_continue_statement` |
| `ast` | method | `NodeFactory.NewElementAccessExpression` | 10 (flamework,jecs) | `NodeFactory::new_element_access_expression` | `rotor_tsrs::ast::NodeFactory::new_element_access_expression` |
| `ast` | method | `NodeFactory.NewEmptyStatement` | 1 (jecs) | `NodeFactory::new_empty_statement` | `rotor_tsrs::ast::NodeFactory::new_empty_statement` |
| `ast` | method | `NodeFactory.NewExpressionStatement` | 8 (flamework,jecs) | `NodeFactory::new_expression_statement` | `rotor_tsrs::ast::NodeFactory::new_expression_statement` |
| `ast` | method | `NodeFactory.NewForInOrOfStatement` | 2 (jecs) | `NodeFactory::new_for_in_or_of_statement` | `rotor_tsrs::ast::NodeFactory::new_for_in_or_of_statement` |
| `ast` | method | `NodeFactory.NewIdentifier` | 38 (flamework,jecs) | `NodeFactory::new_identifier` | `rotor_tsrs::ast::NodeFactory::new_identifier` |
| `ast` | method | `NodeFactory.NewIfStatement` | 3 (jecs) | `NodeFactory::new_if_statement` | `rotor_tsrs::ast::NodeFactory::new_if_statement` |
| `ast` | method | `NodeFactory.NewImportClause` | 3 (flamework) | `NodeFactory::new_import_clause` | `rotor_tsrs::ast::NodeFactory::new_import_clause` |
| `ast` | method | `NodeFactory.NewImportDeclaration` | 4 (flamework) | `NodeFactory::new_import_declaration` | `rotor_tsrs::ast::NodeFactory::new_import_declaration` |
| `ast` | method | `NodeFactory.NewImportSpecifier` | 4 (flamework) | `NodeFactory::new_import_specifier` | `rotor_tsrs::ast::NodeFactory::new_import_specifier` |
| `ast` | method | `NodeFactory.NewImportTypeNode` | 1 (jecs) | `NodeFactory::new_import_type_node` | `rotor_tsrs::ast::NodeFactory::new_import_type_node` |
| `ast` | method | `NodeFactory.NewIndexedAccessTypeNode` | 1 (jecs) | `NodeFactory::new_indexed_access_type_node` | `rotor_tsrs::ast::NodeFactory::new_indexed_access_type_node` |
| `ast` | method | `NodeFactory.NewKeywordExpression` | 2 (flamework) | `NodeFactory::new_keyword_expression` | `rotor_tsrs::ast::NodeFactory::new_keyword_expression` |
| `ast` | method | `NodeFactory.NewKeywordTypeNode` | 3 (flamework,jecs) | `NodeFactory::new_keyword_type_node` | `rotor_tsrs::ast::NodeFactory::new_keyword_type_node` |
| `ast` | method | `NodeFactory.NewLiteralTypeNode` | 4 (flamework,jecs) | `NodeFactory::new_literal_type_node` | `rotor_tsrs::ast::NodeFactory::new_literal_type_node` |
| `ast` | method | `NodeFactory.NewModifierList` | 1 (flamework) | `NodeFactory::new_modifier_list` | `rotor_tsrs::ast::NodeFactory::new_modifier_list` |
| `ast` | method | `NodeFactory.NewNamedImports` | 3 (flamework) | `NodeFactory::new_named_imports` | `rotor_tsrs::ast::NodeFactory::new_named_imports` |
| `ast` | method | `NodeFactory.NewNodeList` | 59 (flamework,jecs) | `NodeFactory::new_node_list` | `rotor_tsrs::ast::NodeFactory::new_node_list` |
| `ast` | method | `NodeFactory.NewNonNullExpression` | 4 (jecs) | `NodeFactory::new_non_null_expression` | `rotor_tsrs::ast::NodeFactory::new_non_null_expression` |
| `ast` | method | `NodeFactory.NewNumericLiteral` | 8 (flamework,jecs) | `NodeFactory::new_numeric_literal` | `rotor_tsrs::ast::NodeFactory::new_numeric_literal` |
| `ast` | method | `NodeFactory.NewObjectLiteralExpression` | 6 (flamework) | `NodeFactory::new_object_literal_expression` | `rotor_tsrs::ast::NodeFactory::new_object_literal_expression` |
| `ast` | method | `NodeFactory.NewParenthesizedExpression` | 1 (jecs) | `NodeFactory::new_parenthesized_expression` | `rotor_tsrs::ast::NodeFactory::new_parenthesized_expression` |
| `ast` | method | `NodeFactory.NewPrefixUnaryExpression` | 3 (flamework,jecs) | `NodeFactory::new_prefix_unary_expression` | `rotor_tsrs::ast::NodeFactory::new_prefix_unary_expression` |
| `ast` | method | `NodeFactory.NewPropertyAccessExpression` | 5 (jecs) | `NodeFactory::new_property_access_expression` | `rotor_tsrs::ast::NodeFactory::new_property_access_expression` |
| `ast` | method | `NodeFactory.NewPropertyAssignment` | 5 (flamework) | `NodeFactory::new_property_assignment` | `rotor_tsrs::ast::NodeFactory::new_property_assignment` |
| `ast` | method | `NodeFactory.NewQualifiedName` | 1 (jecs) | `NodeFactory::new_qualified_name` | `rotor_tsrs::ast::NodeFactory::new_qualified_name` |
| `ast` | method | `NodeFactory.NewStringLiteral` | 45 (flamework,jecs) | `NodeFactory::new_string_literal` | `rotor_tsrs::ast::NodeFactory::new_string_literal` |
| `ast` | method | `NodeFactory.NewSyntaxList` | 4 (flamework,jecs) | `NodeFactory::new_syntax_list` | `rotor_tsrs::ast::NodeFactory::new_syntax_list` |
| `ast` | method | `NodeFactory.NewToken` | 13 (flamework,jecs) | `NodeFactory::new_token` | `rotor_tsrs::ast::NodeFactory::new_token` |
| `ast` | method | `NodeFactory.NewTupleTypeNode` | 1 (jecs) | `NodeFactory::new_tuple_type_node` | `rotor_tsrs::ast::NodeFactory::new_tuple_type_node` |
| `ast` | method | `NodeFactory.NewTypeQueryNode` | 3 (jecs) | `NodeFactory::new_type_query_node` | `rotor_tsrs::ast::NodeFactory::new_type_query_node` |
| `ast` | method | `NodeFactory.NewTypeReferenceNode` | 1 (jecs) | `NodeFactory::new_type_reference_node` | `rotor_tsrs::ast::NodeFactory::new_type_reference_node` |
| `ast` | method | `NodeFactory.NewVariableDeclaration` | 13 (flamework,jecs) | `NodeFactory::new_variable_declaration` | `rotor_tsrs::ast::NodeFactory::new_variable_declaration` |
| `ast` | method | `NodeFactory.NewVariableDeclarationList` | 10 (flamework,jecs) | `NodeFactory::new_variable_declaration_list` | `rotor_tsrs::ast::NodeFactory::new_variable_declaration_list` |
| `ast` | method | `NodeFactory.NewVariableStatement` | 8 (flamework,jecs) | `NodeFactory::new_variable_statement` | `rotor_tsrs::ast::NodeFactory::new_variable_statement` |
| `ast` | method | `NodeFactory.UpdateCallExpression` | 2 (flamework) | `NodeFactory::update_call_expression` | `rotor_tsrs::ast::NodeFactory::update_call_expression` |
| `ast` | method | `NodeFactory.UpdateClassDeclaration` | 1 (flamework) | `NodeFactory::update_class_declaration` | `rotor_tsrs::ast::NodeFactory::update_class_declaration` |
| `ast` | method | `NodeFactory.UpdateElementAccessExpression` | 1 (flamework) | `NodeFactory::update_element_access_expression` | `rotor_tsrs::ast::NodeFactory::update_element_access_expression` |
| `ast` | method | `NodeFactory.UpdateGetAccessorDeclaration` | 1 (flamework) | `NodeFactory::update_get_accessor_declaration` | `rotor_tsrs::ast::NodeFactory::update_get_accessor_declaration` |
| `ast` | method | `NodeFactory.UpdateImportClause` | 1 (flamework) | `NodeFactory::update_import_clause` | `rotor_tsrs::ast::NodeFactory::update_import_clause` |
| `ast` | method | `NodeFactory.UpdateImportDeclaration` | 1 (flamework) | `NodeFactory::update_import_declaration` | `rotor_tsrs::ast::NodeFactory::update_import_declaration` |
| `ast` | method | `NodeFactory.UpdateMethodDeclaration` | 1 (flamework) | `NodeFactory::update_method_declaration` | `rotor_tsrs::ast::NodeFactory::update_method_declaration` |
| `ast` | method | `NodeFactory.UpdateNamedImports` | 1 (flamework) | `NodeFactory::update_named_imports` | `rotor_tsrs::ast::NodeFactory::update_named_imports` |
| `ast` | method | `NodeFactory.UpdateNewExpression` | 2 (flamework) | `NodeFactory::update_new_expression` | `rotor_tsrs::ast::NodeFactory::update_new_expression` |
| `ast` | method | `NodeFactory.UpdateObjectLiteralExpression` | 2 (flamework) | `NodeFactory::update_object_literal_expression` | `rotor_tsrs::ast::NodeFactory::update_object_literal_expression` |
| `ast` | method | `NodeFactory.UpdatePropertyAssignment` | 3 (flamework) | `NodeFactory::update_property_assignment` | `rotor_tsrs::ast::NodeFactory::update_property_assignment` |
| `ast` | method | `NodeFactory.UpdatePropertyDeclaration` | 1 (flamework) | `NodeFactory::update_property_declaration` | `rotor_tsrs::ast::NodeFactory::update_property_declaration` |
| `ast` | method | `NodeFactory.UpdateSetAccessorDeclaration` | 1 (flamework) | `NodeFactory::update_set_accessor_declaration` | `rotor_tsrs::ast::NodeFactory::update_set_accessor_declaration` |
| `ast` | method | `NodeFactory.UpdateSourceFile` | 5 (flamework,jecs) | `NodeFactory::update_source_file` | `rotor_tsrs::ast::NodeFactory::update_source_file` |
| `ast` | method | `NodeVisitor.VisitEachChild` | 11 (flamework,jecs) | `NodeVisitor::visit_each_child` | `rotor_tsrs::ast::NodeVisitor::visit_each_child` |
| `ast` | method | `NodeVisitor.VisitNode` | 1 (flamework) | `NodeVisitor::visit_node` | `rotor_tsrs::ast::NodeVisitor::visit_node` |
| `ast` | method | `NodeVisitor.VisitSourceFile` | 3 (flamework) | `NodeVisitor::visit_source_file` | `rotor_tsrs::ast::NodeVisitor::visit_source_file` |
| `ast` | method | `ParameterDeclaration.Name` | 8 (transformer) | `ParameterDeclaration::name` | `rotor_tsrs::ast::ParameterDeclaration::name` |
| `ast` | method | `PropertyAccessExpression.Name` | 11 (jecs,transformer) | `PropertyAccessExpression::name` | `rotor_tsrs::ast::PropertyAccessExpression::name` |
| `ast` | method | `PropertyAssignment.Name` | 2 (flamework,transformer) | `PropertyAssignment::name` | `rotor_tsrs::ast::PropertyAssignment::name` |
| `ast` | method | `PropertyDeclaration.Name` | 1 (flamework) | `PropertyDeclaration::name` | `rotor_tsrs::ast::PropertyDeclaration::name` |
| `ast` | method | `PropertySignatureDeclaration.Name` | 1 (transformer) | `PropertySignatureDeclaration::name` | `rotor_tsrs::ast::PropertySignatureDeclaration::name` |
| `ast` | method | `SetAccessorDeclaration.Name` | 1 (flamework) | `SetAccessorDeclaration::name` | `rotor_tsrs::ast::SetAccessorDeclaration::name` |
| `ast` | method | `ShorthandPropertyAssignment.Name` | 1 (transformer) | `ShorthandPropertyAssignment::name` | `rotor_tsrs::ast::ShorthandPropertyAssignment::name` |
| `ast` | method | `SourceFile.Diagnostics` | 3 (compile) | `SourceFile::diagnostics` | `rotor_tsrs::ast::SourceFile::diagnostics` |
| `ast` | method | `SourceFile.FileName` | 123 (compile,flamework,transformer) | `SourceFile::file_name` | `rotor_tsrs::ast::SourceFile::file_name` |
| `ast` | method | `SourceFile.Imports` | 3 (compile,flamework) | `SourceFile::imports` | `rotor_tsrs::ast::SourceFile::imports` |
| `ast` | method | `SourceFile.ParseOptions` | 2 (compile) | `SourceFile::parse_options` | `rotor_tsrs::ast::SourceFile::parse_options` |
| `ast` | method | `SourceFile.Path` | 7 (compile) | `SourceFile::path` | `rotor_tsrs::ast::SourceFile::path` |
| `ast` | method | `SourceFile.Text` | 34 (compile,flamework,jecs,transformer) | `SourceFile::text` | `rotor_tsrs::ast::SourceFile::text` |
| `ast` | method | `Symbol.IsExternalModule` | 1 (transformer) | `Symbol::is_external_module` | `rotor_tsrs::ast::Symbol::is_external_module` |
| `ast` | method | `VariableDeclaration.Name` | 8 (transformer) | `VariableDeclaration::name` | `rotor_tsrs::ast::VariableDeclaration::name` |
| `ast` | type | `ConditionalTypeNode` | 1 (transformer) | `ConditionalTypeNode` | `rotor_tsrs::ast::ConditionalTypeNode` |
| `ast` | type | `Diagnostic` | 27 (compile,flamework) | `Diagnostic` | `rotor_tsrs::ast::Diagnostic` |
| `ast` | type | `Kind` | 31 (flamework,jecs,transformer) | `Kind` | `rotor_tsrs::ast::Kind` |
| `ast` | type | `MappedTypeNode` | 2 (transformer) | `MappedTypeNode` | `rotor_tsrs::ast::MappedTypeNode` |
| `ast` | type | `ModifierList` | 2 (flamework) | `ModifierList` | `rotor_tsrs::ast::ModifierList` |
| `ast` | type | `Node` | 1297 (compile,flamework,jecs,transformer) | `Node` | `rotor_tsrs::ast::Node` |
| `ast` | type | `NodeFactory` | 25 (flamework,transformer) | `NodeFactory` | `rotor_tsrs::ast::NodeFactory` |
| `ast` | type | `NodeFlags` | 2 (jecs) | `NodeFlags` | `rotor_tsrs::ast::NodeFlags` |
| `ast` | type | `NodeList` | 1 (jecs) | `NodeList` | `rotor_tsrs::ast::NodeList` |
| `ast` | type | `NodeVisitor` | 6 (flamework,jecs) | `NodeVisitor` | `rotor_tsrs::ast::NodeVisitor` |
| `ast` | type | `NodeVisitorHooks` | 5 (flamework) | `NodeVisitorHooks` | `rotor_tsrs::ast::NodeVisitorHooks` |
| `ast` | type | `SourceFile` | 174 (compile,flamework,jecs,transformer) | `SourceFile` | `rotor_tsrs::ast::SourceFile` |
| `ast` | type | `SourceFileParseOptions` | 18 (compile) | `SourceFileParseOptions` | `rotor_tsrs::ast::SourceFileParseOptions` |
| `ast` | type | `Symbol` | 189 (compile,flamework,jecs,transformer) | `Symbol` | `rotor_tsrs::ast::Symbol` |
| `ast` | type | `TypeNode` | 1 (flamework) | Node (Go alias) | `rotor_tsrs::ast::Node` |
| `bundled` | func | `LibPath` | 1 (compile) | `lib_path` | `rotor_tsrs::vfs::bundled::lib_path` |
| `bundled` | func | `WrapFS` | 2 (compile) | `wrap_fs` | `rotor_tsrs::vfs::bundled::wrap_fs` |
| `checker` | const | `ContextFlagsNone` | 6 (transformer) | `ContextFlags::None` | `rotor_tsrs::checker::ContextFlags::None` |
| `checker` | const | `ElementFlagsOptional` | 1 (transformer) | `ElementFlags::Optional` | `rotor_tsrs::checker::ElementFlags::Optional` |
| `checker` | const | `ElementFlagsRest` | 2 (flamework) | `ElementFlags::Rest` | `rotor_tsrs::checker::ElementFlags::Rest` |
| `checker` | const | `ElementFlagsVariable` | 2 (transformer) | `ElementFlags::Variable` | `rotor_tsrs::checker::ElementFlags::Variable` |
| `checker` | const | `ObjectFlagsClassOrInterface` | 2 (transformer) | `ObjectFlags::ClassOrInterface` | `rotor_tsrs::checker::ObjectFlags::ClassOrInterface` |
| `checker` | const | `ObjectFlagsCouldContainTypeVariables` | 1 (transformer) | `ObjectFlags::CouldContainTypeVariables` | `rotor_tsrs::checker::ObjectFlags::CouldContainTypeVariables` |
| `checker` | const | `ObjectFlagsCouldContainTypeVariablesComputed` | 1 (transformer) | `ObjectFlags::CouldContainTypeVariablesComputed` | `rotor_tsrs::checker::ObjectFlags::CouldContainTypeVariablesComputed` |
| `checker` | const | `ObjectFlagsMapped` | 3 (flamework,transformer) | `ObjectFlags::Mapped` | `rotor_tsrs::checker::ObjectFlags::Mapped` |
| `checker` | const | `ObjectFlagsReference` | 4 (flamework,transformer) | `ObjectFlags::Reference` | `rotor_tsrs::checker::ObjectFlags::Reference` |
| `checker` | const | `ObjectFlagsReverseMapped` | 1 (flamework) | `ObjectFlags::ReverseMapped` | `rotor_tsrs::checker::ObjectFlags::ReverseMapped` |
| `checker` | const | `SignatureKindCall` | 3 (flamework,transformer) | `SignatureKind::Call` | `rotor_tsrs::checker::SignatureKind::Call` |
| `checker` | const | `SignatureKindConstruct` | 1 (flamework) | `SignatureKind::Construct` | `rotor_tsrs::checker::SignatureKind::Construct` |
| `checker` | const | `TypeFlagsAny` | 3 (flamework,transformer) | `TypeFlags::Any` | `rotor_tsrs::checker::TypeFlags::Any` |
| `checker` | const | `TypeFlagsAnyOrUnknown` | 2 (transformer) | `TypeFlags::AnyOrUnknown` | `rotor_tsrs::checker::TypeFlags::AnyOrUnknown` |
| `checker` | const | `TypeFlagsBigInt` | 1 (transformer) | `TypeFlags::BigInt` | `rotor_tsrs::checker::TypeFlags::BigInt` |
| `checker` | const | `TypeFlagsBigIntLiteral` | 1 (transformer) | `TypeFlags::BigIntLiteral` | `rotor_tsrs::checker::TypeFlags::BigIntLiteral` |
| `checker` | const | `TypeFlagsBoolean` | 2 (transformer) | `TypeFlags::Boolean` | `rotor_tsrs::checker::TypeFlags::Boolean` |
| `checker` | const | `TypeFlagsBooleanLiteral` | 7 (flamework,transformer) | `TypeFlags::BooleanLiteral` | `rotor_tsrs::checker::TypeFlags::BooleanLiteral` |
| `checker` | const | `TypeFlagsConditional` | 4 (flamework,transformer) | `TypeFlags::Conditional` | `rotor_tsrs::checker::TypeFlags::Conditional` |
| `checker` | const | `TypeFlagsDisjointDomains` | 1 (flamework) | `TypeFlags::DisjointDomains` | `rotor_tsrs::checker::TypeFlags::DisjointDomains` |
| `checker` | const | `TypeFlagsESSymbolLike` | 3 (flamework,transformer) | `TypeFlags::ESSymbolLike` | `rotor_tsrs::checker::TypeFlags::ESSymbolLike` |
| `checker` | const | `TypeFlagsEnum` | 2 (flamework,transformer) | `TypeFlags::Enum` | `rotor_tsrs::checker::TypeFlags::Enum` |
| `checker` | const | `TypeFlagsEnumLiteral` | 4 (transformer) | `TypeFlags::EnumLiteral` | `rotor_tsrs::checker::TypeFlags::EnumLiteral` |
| `checker` | const | `TypeFlagsIndex` | 1 (transformer) | `TypeFlags::Index` | `rotor_tsrs::checker::TypeFlags::Index` |
| `checker` | const | `TypeFlagsIndexedAccess` | 1 (transformer) | `TypeFlags::IndexedAccess` | `rotor_tsrs::checker::TypeFlags::IndexedAccess` |
| `checker` | const | `TypeFlagsInstantiable` | 4 (flamework,transformer) | `TypeFlags::Instantiable` | `rotor_tsrs::checker::TypeFlags::Instantiable` |
| `checker` | const | `TypeFlagsIntersection` | 1 (flamework) | `TypeFlags::Intersection` | `rotor_tsrs::checker::TypeFlags::Intersection` |
| `checker` | const | `TypeFlagsIntrinsic` | 1 (flamework) | `TypeFlags::Intrinsic` | `rotor_tsrs::checker::TypeFlags::Intrinsic` |
| `checker` | const | `TypeFlagsLiteral` | 2 (flamework,transformer) | `TypeFlags::Literal` | `rotor_tsrs::checker::TypeFlags::Literal` |
| `checker` | const | `TypeFlagsNever` | 6 (flamework,transformer) | `TypeFlags::Never` | `rotor_tsrs::checker::TypeFlags::Never` |
| `checker` | const | `TypeFlagsNonPrimitive` | 2 (transformer) | `TypeFlags::NonPrimitive` | `rotor_tsrs::checker::TypeFlags::NonPrimitive` |
| `checker` | const | `TypeFlagsNull` | 3 (transformer) | `TypeFlags::Null` | `rotor_tsrs::checker::TypeFlags::Null` |
| `checker` | const | `TypeFlagsNumber` | 3 (transformer) | `TypeFlags::Number` | `rotor_tsrs::checker::TypeFlags::Number` |
| `checker` | const | `TypeFlagsNumberLike` | 1 (transformer) | `TypeFlags::NumberLike` | `rotor_tsrs::checker::TypeFlags::NumberLike` |
| `checker` | const | `TypeFlagsNumberLiteral` | 2 (transformer) | `TypeFlags::NumberLiteral` | `rotor_tsrs::checker::TypeFlags::NumberLiteral` |
| `checker` | const | `TypeFlagsObject` | 20 (flamework,transformer) | `TypeFlags::Object` | `rotor_tsrs::checker::TypeFlags::Object` |
| `checker` | const | `TypeFlagsObjectFlagsType` | 2 (transformer) | `TypeFlags::ObjectFlagsType` | `rotor_tsrs::checker::TypeFlags::ObjectFlagsType` |
| `checker` | const | `TypeFlagsString` | 2 (transformer) | `TypeFlags::String` | `rotor_tsrs::checker::TypeFlags::String` |
| `checker` | const | `TypeFlagsStringLike` | 2 (transformer) | `TypeFlags::StringLike` | `rotor_tsrs::checker::TypeFlags::StringLike` |
| `checker` | const | `TypeFlagsStringLiteral` | 2 (transformer) | `TypeFlags::StringLiteral` | `rotor_tsrs::checker::TypeFlags::StringLiteral` |
| `checker` | const | `TypeFlagsStringMapping` | 1 (transformer) | `TypeFlags::StringMapping` | `rotor_tsrs::checker::TypeFlags::StringMapping` |
| `checker` | const | `TypeFlagsSubstitution` | 1 (transformer) | `TypeFlags::Substitution` | `rotor_tsrs::checker::TypeFlags::Substitution` |
| `checker` | const | `TypeFlagsTemplateLiteral` | 3 (flamework,transformer) | `TypeFlags::TemplateLiteral` | `rotor_tsrs::checker::TypeFlags::TemplateLiteral` |
| `checker` | const | `TypeFlagsTypeParameter` | 2 (flamework,transformer) | `TypeFlags::TypeParameter` | `rotor_tsrs::checker::TypeFlags::TypeParameter` |
| `checker` | const | `TypeFlagsTypeVariable` | 2 (flamework,transformer) | `TypeFlags::TypeVariable` | `rotor_tsrs::checker::TypeFlags::TypeVariable` |
| `checker` | const | `TypeFlagsUndefined` | 2 (flamework,transformer) | `TypeFlags::Undefined` | `rotor_tsrs::checker::TypeFlags::Undefined` |
| `checker` | const | `TypeFlagsUnion` | 1 (flamework) | `TypeFlags::Union` | `rotor_tsrs::checker::TypeFlags::Union` |
| `checker` | const | `TypeFlagsUnionOrIntersection` | 8 (flamework,transformer) | `TypeFlags::UnionOrIntersection` | `rotor_tsrs::checker::TypeFlags::UnionOrIntersection` |
| `checker` | const | `TypeFlagsUniqueESSymbol` | 1 (flamework) | `TypeFlags::UniqueESSymbol` | `rotor_tsrs::checker::TypeFlags::UniqueESSymbol` |
| `checker` | const | `TypeFlagsUnknown` | 3 (flamework,transformer) | `TypeFlags::Unknown` | `rotor_tsrs::checker::TypeFlags::Unknown` |
| `checker` | const | `TypeFlagsVoid` | 3 (transformer) | `TypeFlags::Void` | `rotor_tsrs::checker::TypeFlags::Void` |
| `checker` | const | `TypeFlagsVoidLike` | 2 (transformer) | `TypeFlags::VoidLike` | `rotor_tsrs::checker::TypeFlags::VoidLike` |
| `checker` | func | `GetDeclarationModifierFlagsFromSymbol` | 3 (transformer) | `get_declaration_modifier_flags_from_symbol_exported` | `rotor_tsrs::checker::get_declaration_modifier_flags_from_symbol` |
| `checker` | func | `IsTupleType` | 7 (flamework) | `is_tuple_type_exported` | `rotor_tsrs::checker::is_tuple_type` |
| `checker` | func | `SkipAlias` | 11 (transformer) | `skip_alias` | `rotor_tsrs::checker::skip_alias` |
| `checker` | method | `Checker.GetAliasedSymbol` | 14 (flamework,jecs,transformer) | `Checker::get_aliased_symbol` | `rotor_tsrs::checker::TypeChecker::get_aliased_symbol` |
| `checker` | method | `Checker.GetAmbientModules` | 1 (compile) | `Checker::get_ambient_modules` | `rotor_tsrs::checker::TypeChecker::get_ambient_modules` |
| `checker` | method | `Checker.GetAnyType` | 3 (flamework,transformer) | `Checker::get_any_type` | `rotor_tsrs::checker::TypeChecker::get_any_type` |
| `checker` | method | `Checker.GetApparentProperties` | 1 (flamework) | `Checker::get_apparent_properties` | `rotor_tsrs::checker::TypeChecker::get_apparent_properties` |
| `checker` | method | `Checker.GetApparentType` | 2 (transformer) | `Checker::get_apparent_type_exported` | `rotor_tsrs::checker::TypeChecker::get_apparent_type` |
| `checker` | method | `Checker.GetBaseConstraintOfType` | 6 (flamework,transformer) | `Checker::get_base_constraint_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_base_constraint_of_type` |
| `checker` | method | `Checker.GetBaseTypeOfLiteralType` | 1 (transformer) | `Checker::get_base_type_of_literal_type_exported` | `rotor_tsrs::checker::TypeChecker::get_base_type_of_literal_type` |
| `checker` | method | `Checker.GetBaseTypes` | 2 (transformer) | `Checker::get_base_types_exported` | `rotor_tsrs::checker::TypeChecker::get_base_types` |
| `checker` | method | `Checker.GetBooleanType` | 2 (flamework) | `Checker::get_boolean_type` | `rotor_tsrs::checker::TypeChecker::get_boolean_type` |
| `checker` | method | `Checker.GetCallSignatures` | 13 (flamework,transformer) | `Checker::get_call_signatures` | `rotor_tsrs::checker::TypeChecker::get_call_signatures` |
| `checker` | method | `Checker.GetConstantValue` | 6 (flamework,transformer) | `Checker::get_constant_value` | `rotor_tsrs::checker::TypeChecker::get_constant_value` |
| `checker` | method | `Checker.GetConstraintOfTypeParameter` | 1 (transformer) | `Checker::get_constraint_of_type_parameter_exported` | `rotor_tsrs::checker::TypeChecker::get_constraint_of_type_parameter` |
| `checker` | method | `Checker.GetConstructSignatures` | 7 (flamework,transformer) | Checker::get_signatures_of_type_exported(t, Construct) | `rotor_tsrs::checker::TypeChecker::get_construct_signatures` |
| `checker` | method | `Checker.GetContextualType` | 5 (transformer) | `Checker::get_contextual_type_exported` | `rotor_tsrs::checker::TypeChecker::get_contextual_type` |
| `checker` | method | `Checker.GetContextualTypeForObjectLiteralElement` | 1 (transformer) | `Checker::get_contextual_type_for_object_literal_element_exported` | `rotor_tsrs::checker::TypeChecker::get_contextual_type_for_object_literal_element` |
| `checker` | method | `Checker.GetDeclaredTypeOfSymbol` | 17 (flamework,jecs,transformer) | `Checker::get_declared_type_of_symbol_exported` | `rotor_tsrs::checker::TypeChecker::get_declared_type_of_symbol` |
| `checker` | method | `Checker.GetDefaultFromTypeParameter` | 3 (flamework,transformer) | `Checker::get_default_from_type_parameter_exported` | `rotor_tsrs::checker::TypeChecker::get_default_from_type_parameter` |
| `checker` | method | `Checker.GetElementTypeOfArrayType` | 5 (transformer) | `Checker::get_element_type_of_array_type_exported` | `rotor_tsrs::checker::TypeChecker::get_element_type_of_array_type` |
| `checker` | method | `Checker.GetEmitResolver` | 1 (transformer) | Checker::get_emit_resolver (0001 patch) | `rotor_tsrs::checker::TypeChecker::get_emit_resolver` |
| `checker` | method | `Checker.GetExportSpecifierLocalTargetSymbol` | 1 (transformer) | `Checker::get_export_specifier_local_target_symbol` | `rotor_tsrs::checker::TypeChecker::get_export_specifier_local_target_symbol` |
| `checker` | method | `Checker.GetExportsOfModule` | 2 (jecs,transformer) | `Checker::get_exports_of_module_exported` | `rotor_tsrs::checker::TypeChecker::get_exports_of_module` |
| `checker` | method | `Checker.GetGlobalSymbol` | 2 (transformer) | `Checker::get_global_symbol_exported` | `rotor_tsrs::checker::TypeChecker::get_global_symbol` |
| `checker` | method | `Checker.GetIndexInfosOfType` | 6 (flamework,transformer) | `Checker::get_index_infos_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_index_infos_of_type` |
| `checker` | method | `Checker.GetIndexTypeOfType` | 1 (transformer) | Checker::get_index_type_of_type (0001 patch) | `rotor_tsrs::checker::TypeChecker::get_index_type_of_type` |
| `checker` | method | `Checker.GetLocalTypeParametersOfClassOrInterfaceOrTypeAlias` | 2 (transformer) | `Checker::get_local_type_parameters_of_class_or_interface_or_type_alias_exported` | `rotor_tsrs::checker::TypeChecker::get_local_type_parameters_of_class_or_interface_or_type_alias` |
| `checker` | method | `Checker.GetMergedSymbol` | 7 (flamework,transformer) | `Checker::get_merged_symbol_exported` | `rotor_tsrs::checker::TypeChecker::get_merged_symbol` |
| `checker` | method | `Checker.GetNonNullableType` | 6 (flamework,transformer) | `Checker::get_non_nullable_type` | `rotor_tsrs::checker::TypeChecker::get_non_nullable_type` |
| `checker` | method | `Checker.GetNonOptionalType` | 11 (transformer) | `Checker::get_non_optional_type` | `rotor_tsrs::checker::TypeChecker::get_non_optional_type` |
| `checker` | method | `Checker.GetNumberIndexType` | 3 (flamework,transformer) | `Checker::get_number_index_type` | `rotor_tsrs::checker::TypeChecker::get_number_index_type` |
| `checker` | method | `Checker.GetNumberType` | 2 (flamework,transformer) | `Checker::get_number_type` | `rotor_tsrs::checker::TypeChecker::get_number_type` |
| `checker` | method | `Checker.GetPromisedTypeOfPromise` | 1 (transformer) | `Checker::get_promised_type_of_promise` | `rotor_tsrs::checker::TypeChecker::get_promised_type_of_promise` |
| `checker` | method | `Checker.GetPropertiesOfType` | 19 (flamework,transformer) | `Checker::get_properties_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_properties_of_type` |
| `checker` | method | `Checker.GetPropertyOfType` | 16 (flamework,jecs,transformer) | `Checker::get_property_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_property_of_type` |
| `checker` | method | `Checker.GetResolvedSignature` | 8 (flamework,jecs) | `Checker::get_resolved_signature_exported` | `rotor_tsrs::checker::TypeChecker::get_resolved_signature` |
| `checker` | method | `Checker.GetReturnTypeOfSignature` | 8 (flamework,jecs,transformer) | `Checker::get_return_type_of_signature_exported` | `rotor_tsrs::checker::TypeChecker::get_return_type_of_signature` |
| `checker` | method | `Checker.GetShorthandAssignmentValueSymbol` | 3 (transformer) | `Checker::get_shorthand_assignment_value_symbol` | `rotor_tsrs::checker::TypeChecker::get_shorthand_assignment_value_symbol` |
| `checker` | method | `Checker.GetSignatureFromDeclaration` | 3 (flamework,transformer) | `Checker::get_signature_from_declaration_exported` | `rotor_tsrs::checker::TypeChecker::get_signature_from_declaration` |
| `checker` | method | `Checker.GetSignaturesOfType` | 3 (flamework,transformer) | `Checker::get_signatures_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_signatures_of_type` |
| `checker` | method | `Checker.GetStringIndexType` | 2 (flamework,transformer) | `Checker::get_string_index_type` | `rotor_tsrs::checker::TypeChecker::get_string_index_type` |
| `checker` | method | `Checker.GetStringType` | 1 (flamework) | `Checker::get_string_type` | `rotor_tsrs::checker::TypeChecker::get_string_type` |
| `checker` | method | `Checker.GetSymbolAtLocation` | 74 (compile,flamework,jecs,transformer) | `Checker::get_symbol_at_location_exported` | `rotor_tsrs::checker::TypeChecker::get_symbol_at_location` |
| `checker` | method | `Checker.GetSymbolsInScope` | 1 (jecs) | `Checker::get_symbols_in_scope_exported` | `rotor_tsrs::checker::TypeChecker::get_symbols_in_scope` |
| `checker` | method | `Checker.GetTypeArguments` | 7 (flamework,transformer) | `Checker::get_type_arguments_exported` | `rotor_tsrs::checker::TypeChecker::get_type_arguments` |
| `checker` | method | `Checker.GetTypeAtLocation` | 28 (flamework,transformer) | `Checker::get_type_at_location` | `rotor_tsrs::checker::TypeChecker::get_type_at_location` |
| `checker` | method | `Checker.GetTypeAtPosition` | 2 (flamework) | `Checker::get_type_at_position_exported` | `rotor_tsrs::checker::TypeChecker::get_type_at_position` |
| `checker` | method | `Checker.GetTypeFromTypeNode` | 12 (flamework,transformer) | `Checker::get_type_from_type_node_exported` | `rotor_tsrs::checker::TypeChecker::get_type_from_type_node` |
| `checker` | method | `Checker.GetTypeOfAssignmentPattern` | 3 (transformer) | Checker::get_type_of_assignment_pattern (0001) + error_type (0002) | `rotor_tsrs::checker::TypeChecker::get_type_of_assignment_pattern` |
| `checker` | method | `Checker.GetTypeOfPropertyOfType` | 23 (flamework,transformer) | `Checker::get_type_of_property_of_type_exported` | `rotor_tsrs::checker::TypeChecker::get_type_of_property_of_type` |
| `checker` | method | `Checker.GetTypeOfSymbol` | 11 (flamework,transformer) | `Checker::get_type_of_symbol_exported` | `rotor_tsrs::checker::TypeChecker::get_type_of_symbol` |
| `checker` | method | `Checker.GetTypeOfSymbolAtLocation` | 7 (flamework,transformer) | `Checker::get_type_of_symbol_at_location` | `rotor_tsrs::checker::TypeChecker::get_type_of_symbol_at_location` |
| `checker` | method | `Checker.GetUndefinedType` | 2 (flamework) | `Checker::get_undefined_type` | `rotor_tsrs::checker::TypeChecker::get_undefined_type` |
| `checker` | method | `Checker.GetVoidType` | 4 (flamework,transformer) | `Checker::get_void_type` | `rotor_tsrs::checker::TypeChecker::get_void_type` |
| `checker` | method | `Checker.GetWidenedType` | 1 (transformer) | `Checker::get_widened_type_exported` | `rotor_tsrs::checker::TypeChecker::get_widened_type` |
| `checker` | method | `Checker.IsArgumentsSymbol` | 1 (transformer) | `Checker::is_arguments_symbol` | `rotor_tsrs::checker::TypeChecker::is_arguments_symbol` |
| `checker` | method | `Checker.IsArrayLikeType` | 2 (flamework,transformer) | `Checker::is_array_like_type_exported` | `rotor_tsrs::checker::TypeChecker::is_array_like_type` |
| `checker` | method | `Checker.IsTypeAssignableTo` | 6 (flamework,transformer) | `Checker::is_type_assignable_to_exported` | `rotor_tsrs::checker::TypeChecker::is_type_assignable_to` |
| `checker` | method | `Checker.IsUndefinedSymbol` | 2 (transformer) | `Checker::is_undefined_symbol` | `rotor_tsrs::checker::TypeChecker::is_undefined_symbol` |
| `checker` | method | `Checker.ResolveExternalModuleName` | 1 (transformer) | `Checker::resolve_external_module_name_exported` | `rotor_tsrs::checker::TypeChecker::resolve_external_module_name` |
| `checker` | method | `Checker.ResolveName` | 13 (flamework,transformer) | `Checker::resolve_name_exported` | `rotor_tsrs::checker::TypeChecker::resolve_name` |
| `checker` | method | `Checker.TypeToString` | 20 (flamework,transformer) | `Checker::type_to_string_exported` | `rotor_tsrs::checker::TypeChecker::type_to_string` |
| `checker` | method | `Checker.TypeToTypeNode` | 1 (flamework) | `Checker::type_to_type_node` | `rotor_tsrs::checker::TypeChecker::type_to_type_node` |
| `checker` | method | `ConditionalType.ExtendsType` | 1 (transformer) | `ConditionalType::extends_type` | `rotor_tsrs::checker::ConditionalType::extends_type` |
| `checker` | method | `ConditionalType.RootCheckType` | 1 (transformer) | ConditionalType.root.check_type (0001 patch) | `rotor_tsrs::checker::conditional_root_check_type` |
| `checker` | method | `ConditionalType.RootNode` | 1 (transformer) | ConditionalType.root.node (0001 patch) | `rotor_tsrs::checker::conditional_root_node` |
| `checker` | method | `EmitResolver.GetJsxFactoryEntityUnsafe` | 3 (transformer) | `EmitResolver::get_jsx_factory_entity` | `rotor_tsrs::checker::TypeChecker::get_jsx_factory_entity` |
| `checker` | method | `EmitResolver.GetJsxFragmentFactoryEntityUnsafe` | 2 (transformer) | `EmitResolver::get_jsx_fragment_factory_entity` | `rotor_tsrs::checker::TypeChecker::get_jsx_fragment_factory_entity` |
| `checker` | method | `EmitResolver.IsReferencedAliasDeclarationUnsafe` | 2 (transformer) | `EmitResolver::is_referenced_alias_declaration` | `rotor_tsrs::checker::TypeChecker::is_referenced_alias_declaration` |
| `checker` | method | `IndexInfo.IsReadonly` | 3 (transformer) | `IndexInfo::is_readonly` | `rotor_tsrs::checker::IndexInfo::is_readonly` |
| `checker` | method | `IndexInfo.KeyType` | 4 (flamework,transformer) | `IndexInfo::key_type` | `rotor_tsrs::checker::IndexInfo::key_type` |
| `checker` | method | `IndexInfo.ValueType` | 6 (flamework,transformer) | `IndexInfo::value_type` | `rotor_tsrs::checker::IndexInfo::value_type` |
| `checker` | method | `IndexType.Target` | 1 (transformer) | `IndexType::target` | `rotor_tsrs::checker::IndexType::target` |
| `checker` | method | `IndexedAccessType.IndexType` | 1 (transformer) | `IndexedAccessType::index_type` | `rotor_tsrs::checker::IndexedAccessType::index_type` |
| `checker` | method | `IndexedAccessType.ObjectType` | 1 (transformer) | `IndexedAccessType::object_type` | `rotor_tsrs::checker::IndexedAccessType::object_type` |
| `checker` | method | `IntrinsicType.IntrinsicName` | 1 (flamework) | `IntrinsicType::intrinsic_name` | `rotor_tsrs::checker::IntrinsicType::intrinsic_name` |
| `checker` | method | `LiteralType.RegularType` | 1 (transformer) | `LiteralType::regular_type` | `rotor_tsrs::checker::LiteralType::regular_type` |
| `checker` | method | `LiteralType.String` | 2 (flamework) | `LiteralType::is_string` | `rotor_tsrs::checker::LiteralType::is_string` |
| `checker` | method | `LiteralType.Value` | 9 (flamework,transformer) | `LiteralType::value` | `rotor_tsrs::checker::LiteralType::value` |
| `checker` | method | `Signature.Declaration` | 8 (flamework,transformer) | `Signature::declaration` | `rotor_tsrs::checker::Signature::declaration` |
| `checker` | method | `Signature.HasRestParameter` | 3 (flamework,transformer) | `Signature::has_rest_parameter` | `rotor_tsrs::checker::Signature::has_rest_parameter` |
| `checker` | method | `Signature.MinArgumentCount` | 2 (transformer) | `Signature::min_argument_count` | `rotor_tsrs::checker::Signature::min_argument_count` |
| `checker` | method | `Signature.Parameters` | 7 (flamework,transformer) | `Signature::parameters` | `rotor_tsrs::checker::Signature::parameters` |
| `checker` | method | `Signature.ThisParameter` | 3 (transformer) | `Signature::this_parameter` | `rotor_tsrs::checker::Signature::this_parameter` |
| `checker` | method | `Signature.TypeParameters` | 1 (transformer) | `Signature::type_parameters` | `rotor_tsrs::checker::Signature::type_parameters` |
| `checker` | method | `SubstitutionType.BaseType` | 1 (transformer) | `SubstitutionType::base_type` | `rotor_tsrs::checker::SubstitutionType::base_type` |
| `checker` | method | `TemplateLiteralType.Texts` | 1 (transformer) | `TemplateLiteralType::texts` | `rotor_tsrs::checker::TemplateLiteralType::texts` |
| `checker` | method | `TupleElementInfo.LabeledDeclaration` | 4 (flamework,transformer) | `TupleElementInfo::labeled_declaration` | `rotor_tsrs::checker::TupleElementInfo::labeled_declaration` |
| `checker` | method | `TupleElementInfo.TupleElementFlags` | 3 (flamework,transformer) | TupleElementInfo::flags (0002 patch) | `rotor_tsrs::checker::TupleElementInfo::flags` |
| `checker` | method | `TupleType.ElementFlags` | 2 (flamework,transformer) | `TupleType::element_flags` | `rotor_tsrs::checker::TupleType::element_flags` |
| `checker` | method | `TupleType.ElementInfos` | 5 (flamework,transformer) | `TupleType::element_infos` | `rotor_tsrs::checker::TupleType::element_infos` |
| `checker` | method | `TupleType.FixedLength` | 1 (flamework) | `TupleType::fixed_length` | `rotor_tsrs::checker::TupleType::fixed_length` |
| `checker` | method | `TupleType.IsReadonly` | 1 (transformer) | `TupleType::is_readonly` | `rotor_tsrs::checker::TupleType::is_readonly` |
| `checker` | method | `Type.Alias` | 15 (flamework,jecs,transformer) | `Type::alias` | `rotor_tsrs::checker::Type::alias` |
| `checker` | method | `Type.AsConditionalType` | 1 (transformer) | `Type::as_conditional_type` | `rotor_tsrs::checker::Type::as_conditional_type` |
| `checker` | method | `Type.AsIndexType` | 1 (transformer) | `Type::as_index_type` | `rotor_tsrs::checker::Type::as_index_type` |
| `checker` | method | `Type.AsIndexedAccessType` | 1 (transformer) | `Type::as_indexed_access_type` | `rotor_tsrs::checker::Type::as_indexed_access_type` |
| `checker` | method | `Type.AsIntrinsicType` | 1 (flamework) | `Type::as_intrinsic_type` | `rotor_tsrs::checker::Type::as_intrinsic_type` |
| `checker` | method | `Type.AsLiteralType` | 11 (flamework,transformer) | `Type::as_literal_type` | `rotor_tsrs::checker::Type::as_literal_type` |
| `checker` | method | `Type.AsSubstitutionType` | 1 (transformer) | `Type::as_substitution_type` | `rotor_tsrs::checker::Type::as_substitution_type` |
| `checker` | method | `Type.AsTemplateLiteralType` | 1 (transformer) | `Type::as_template_literal_type` | `rotor_tsrs::checker::Type::as_template_literal_type` |
| `checker` | method | `Type.AsTypeParameter` | 1 (transformer) | `Type::as_type_parameter` | `rotor_tsrs::checker::Type::as_type_parameter` |
| `checker` | method | `Type.Distributed` | 2 (flamework) | none; three-line helper | `rotor_tsrs::checker::distributed` |
| `checker` | method | `Type.Flags` | 70 (flamework,transformer) | `Type::flags` | `rotor_tsrs::checker::Type::flags` |
| `checker` | method | `Type.Id` | 2 (flamework) | `Type::id` | `rotor_tsrs::checker::Type::id` |
| `checker` | method | `Type.IsClass` | 1 (flamework) | `Type::is_class` | `rotor_tsrs::checker::Type::is_class` |
| `checker` | method | `Type.IsIntersection` | 6 (flamework,transformer) | `Type::is_intersection` | `rotor_tsrs::checker::Type::is_intersection` |
| `checker` | method | `Type.IsNumberLiteral` | 4 (flamework,transformer) | `Type::is_number_literal` | `rotor_tsrs::checker::Type::is_number_literal` |
| `checker` | method | `Type.IsStringLiteral` | 14 (flamework,transformer) | `Type::is_string_literal` | `rotor_tsrs::checker::Type::is_string_literal` |
| `checker` | method | `Type.IsTupleType` | 8 (transformer) | `Type::is_tuple_type` | `rotor_tsrs::checker::Type::is_tuple_type` |
| `checker` | method | `Type.IsUnion` | 10 (flamework,transformer) | `Type::is_union` | `rotor_tsrs::checker::Type::is_union` |
| `checker` | method | `Type.ObjectFlags` | 8 (flamework,transformer) | `Type::object_flags` | `rotor_tsrs::checker::Type::object_flags` |
| `checker` | method | `Type.Symbol` | 42 (flamework,transformer) | `Type::symbol` | `rotor_tsrs::checker::Type::symbol` |
| `checker` | method | `Type.Target` | 1 (flamework) | `Type::target` | `rotor_tsrs::checker::Type::target` |
| `checker` | method | `Type.TargetTupleType` | 8 (flamework,transformer) | `Type::target_tuple_type` | `rotor_tsrs::checker::Type::target_tuple_type` |
| `checker` | method | `Type.Types` | 21 (flamework,transformer) | `Type::types` | `rotor_tsrs::checker::Type::types` |
| `checker` | method | `TypeAlias.Symbol` | 13 (flamework,jecs,transformer) | `TypeAlias::symbol` | `rotor_tsrs::checker::TypeAlias::symbol` |
| `checker` | method | `TypeAlias.TypeArguments` | 6 (transformer) | `TypeAlias::type_arguments` | `rotor_tsrs::checker::TypeAlias::type_arguments` |
| `checker` | method | `TypeParameter.IsThisType` | 1 (transformer) | `TypeParameter::is_this_type` | `rotor_tsrs::checker::TypeParameter::is_this_type` |
| `checker` | type | `Checker` | 20 (compile,flamework,jecs,transformer) | `Checker` | `rotor_tsrs::checker::Checker` |
| `checker` | type | `ElementFlags` | 2 (transformer) | `ElementFlags` | `rotor_tsrs::checker::ElementFlags` |
| `checker` | type | `EmitResolver` | 1 (transformer) | `EmitResolver` | `rotor_tsrs::checker::EmitResolver` |
| `checker` | type | `IndexInfo` | 2 (transformer) | `IndexInfo` | `rotor_tsrs::checker::IndexInfo` |
| `checker` | type | `Signature` | 9 (flamework,transformer) | `Signature` | `rotor_tsrs::checker::Signature` |
| `checker` | type | `TupleType` | 1 (transformer) | `TupleType` | `rotor_tsrs::checker::TupleType` |
| `checker` | type | `Type` | 231 (flamework,jecs,transformer) | `Type` | `rotor_tsrs::checker::Type` |
| `checker` | type | `TypeId` | 1 (flamework) | `TypeId` | `rotor_tsrs::checker::TypeId` |
| `collections` | method | `OrderedMap.Entries` | 2 (compile,transformer) | OrderedMap = IndexMap: iter() | `rotor_tsrs::tscore::collections::OrderedMap::iter` |
| `collections` | method | `OrderedMap.Set` | 1 (compile) | `OrderedMap::set` | `rotor_tsrs::tscore::collections::OrderedMap::set` |
| `collections` | method | `OrderedMap.Size` | 1 (compile) | `OrderedMap::size` | `rotor_tsrs::tscore::collections::OrderedMap::size` |
| `collections` | method | `SyncMap.Load` | 1 (transformer) | `SyncMap::load` | `rotor_tsrs::tscore::collections::SyncMap::load` |
| `collections` | method | `SyncMap.Range` | 2 (compile) | `SyncMap::range` | `rotor_tsrs::tscore::collections::SyncMap::range` |
| `collections` | method | `SyncSet.Range` | 1 (compile) | `SyncSet::range` | `rotor_tsrs::tscore::collections::SyncSet::range` |
| `collections` | method | `SyncSet.ToSlice` | 1 (transformer) | `SyncSet::to_slice` | `rotor_tsrs::tscore::collections::SyncSet::to_slice` |
| `collections` | type | `OrderedMap` | 1 (compile) | `OrderedMap` | `rotor_tsrs::tscore::collections::OrderedMap` |
| `collections` | type | `SyncSet` | 1 (compile) | `SyncSet` | `rotor_tsrs::tscore::collections::SyncSet` |
| `compiler` | const | `EmitOnlyDts` | 1 (compile) | `EmitOnly::EmitOnlyDts` | `rotor_tsrs::compiler::emitter::EmitOnly::EmitOnlyDts` |
| `compiler` | field | `EmitOptions.EmitOnly` | 1 (compile) | `EmitOptions::emit_only` | `rotor_tsrs::compiler::EmitOptions::emit_only` |
| `compiler` | field | `EmitOptions.TargetSourceFile` | 1 (compile) | EmitOptions::target_source_files (a list) | `rotor_tsrs::compiler::EmitOptions::target_source_files` |
| `compiler` | field | `EmitOptions.WriteFile` | 1 (compile) | `EmitOptions::write_file` | `rotor_tsrs::compiler::EmitOptions::write_file` |
| `compiler` | field | `EmitResult.Diagnostics` | 2 (compile) | `EmitResult::diagnostics` | `rotor_tsrs::compiler::EmitResult::diagnostics` |
| `compiler` | field | `ProgramOptions.Config` | 1 (compile) | ProgramOptions::new(config, host) | `rotor_tsrs::project::load_project` |
| `compiler` | field | `ProgramOptions.Host` | 1 (compile) | `ProgramOptions::host` | `rotor_tsrs::compiler::ProgramOptions::host` |
| `compiler` | field | `WriteFileData.SkippedDtsWrite` | 1 (compile) | `WriteFileData::skipped_dts_write` | `rotor_tsrs::compiler::WriteFileData::skipped_dts_write` |
| `compiler` | func | `NewCompilerHost` | 1 (compile) | `new_cached_fs_compiler_host` | `rotor_tsrs::project::load_project` |
| `compiler` | func | `NewProgram` | 1 (compile) | `new_program` | `rotor_tsrs::project::load_project` |
| `compiler` | method | `CompilerHost.FS` | 10 (compile,transformer) | `CompilerHost::fs` | `rotor_tsrs::compiler::CompilerHost::fs` |
| `compiler` | method | `CompilerHost.GetCurrentDirectory` | 1 (compile) | `CompilerHost::get_current_directory` | `rotor_tsrs::compiler::CompilerHost::get_current_directory` |
| `compiler` | method | `CompilerHost.GetSourceFile` | 3 (compile) | `CompilerHost::get_source_file` | `rotor_tsrs::compiler::CompilerHost::get_source_file` |
| `compiler` | method | `Program.CommandLine` | 4 (compile) | `Program::command_line` | `rotor_tsrs::compiler::Program::command_line` |
| `compiler` | method | `Program.Emit` | 1 (compile) | `Program::emit` | `rotor_tsrs::compiler::Program::emit` |
| `compiler` | method | `Program.ForEachCheckerParallel` | 4 (compile,flamework) | `Program::for_each_checker_parallel` | `rotor_tsrs::compiler::Program::for_each_checker_parallel` |
| `compiler` | method | `Program.ForEachFileOnFreeChecker` | 1 (compile) | — | missing: rotor's tsgo checker-pool patch; tsrs's pool already steals work inside for_each_checker_group, so Stage 5 decides whether it is needed |
| `compiler` | method | `Program.GetCurrentDirectory` | 8 (compile,transformer) | `Program::get_current_directory` | `rotor_tsrs::compiler::Program::get_current_directory` |
| `compiler` | method | `Program.GetDefaultResolutionModeForFile` | 1 (compile) | `Program::get_default_resolution_mode_for_file` | `rotor_tsrs::compiler::Program::get_default_resolution_mode_for_file` |
| `compiler` | method | `Program.GetGlobalDiagnostics` | 2 (compile) | `Program::get_global_diagnostics` | `rotor_tsrs::compiler::Program::get_global_diagnostics` |
| `compiler` | method | `Program.GetModeForUsageLocation` | 1 (transformer) | `Program::get_mode_for_usage_location` | `rotor_tsrs::compiler::Program::get_mode_for_usage_location` |
| `compiler` | method | `Program.GetParseFileRedirect` | 1 (compile) | `Program::get_parse_file_redirect` | `rotor_tsrs::compiler::Program::get_parse_file_redirect` |
| `compiler` | method | `Program.GetProgramDiagnostics` | 2 (compile) | `Program::get_program_diagnostics` | `rotor_tsrs::compiler::Program::get_program_diagnostics` |
| `compiler` | method | `Program.GetRedirectTargets` | 1 (compile) | Program::get_redirect_targets (0002 patch) | `rotor_tsrs::compiler::Program::get_redirect_targets` |
| `compiler` | method | `Program.GetResolvedModule` | 2 (jecs,transformer) | `Program::get_resolved_module` | `rotor_tsrs::compiler::Program::get_resolved_module` |
| `compiler` | method | `Program.GetResolvedModuleFromModuleSpecifier` | 1 (flamework) | `Program::get_resolved_module_from_module_specifier` | `rotor_tsrs::compiler::Program::get_resolved_module_from_module_specifier` |
| `compiler` | method | `Program.GetResolvedModules` | 2 (compile,jecs) | `Program::get_resolved_modules` | `rotor_tsrs::compiler::Program::get_resolved_modules` |
| `compiler` | method | `Program.GetResolvedProjectReferenceFor` | 1 (compile) | — | missing: not in tsrs 0.20.1; Stage 5 can find it in Program::get_resolved_project_references |
| `compiler` | method | `Program.GetResolvedProjectReferences` | 1 (compile) | `Program::get_resolved_project_references` | `rotor_tsrs::compiler::Program::get_resolved_project_references` |
| `compiler` | method | `Program.GetResolvedTypeReferenceDirectives` | 1 (compile) | `Program::get_resolved_type_reference_directives` | `rotor_tsrs::compiler::Program::get_resolved_type_reference_directives` |
| `compiler` | method | `Program.GetSemanticDiagnostics` | 1 (compile) | `Program::get_semantic_diagnostics` | `rotor_tsrs::compiler::Program::get_semantic_diagnostics` |
| `compiler` | method | `Program.GetSourceFile` | 10 (compile,flamework,jecs,transformer) | `Program::get_source_file` | `rotor_tsrs::compiler::Program::get_source_file` |
| `compiler` | method | `Program.GetSourceFileByPath` | 1 (compile) | `Program::get_source_file_by_path` | `rotor_tsrs::compiler::Program::get_source_file_by_path` |
| `compiler` | method | `Program.GetSourceFiles` | 3 (flamework) | `Program::get_source_files` | `rotor_tsrs::compiler::Program::get_source_files` |
| `compiler` | method | `Program.GetSymlinkCache` | 2 (compile,transformer) | `Program::get_symlink_cache` | `rotor_tsrs::compiler::Program::get_symlink_cache` |
| `compiler` | method | `Program.GetSyntacticDiagnostics` | 1 (compile) | `Program::get_syntactic_diagnostics` | `rotor_tsrs::compiler::Program::get_syntactic_diagnostics` |
| `compiler` | method | `Program.GetTypeChecker` | 3 (compile,flamework) | `Program::get_type_checker` | `rotor_tsrs::compiler::Program::get_type_checker` |
| `compiler` | method | `Program.GetTypeCheckerForFile` | 2 (compile) | `Program::get_type_checker_for_file` | `rotor_tsrs::compiler::Program::get_type_checker_for_file` |
| `compiler` | method | `Program.GetTypeCheckerForFileExclusive` | 2 (compile) | `Program::get_type_checker_for_file_exclusive` | `rotor_tsrs::compiler::Program::get_type_checker_for_file_exclusive` |
| `compiler` | method | `Program.Host` | 8 (compile,transformer) | `Program::host` | `rotor_tsrs::compiler::Program::host` |
| `compiler` | method | `Program.IsLibFile` | 1 (transformer) | `Program::is_lib_file` | `rotor_tsrs::compiler::Program::is_lib_file` |
| `compiler` | method | `Program.IsSourceFileFromExternalLibrary` | 3 (compile,flamework) | `Program::is_source_file_from_external_library` | `rotor_tsrs::compiler::Program::is_source_file_from_external_library` |
| `compiler` | method | `Program.IsSourceFromProjectReference` | 2 (compile) | `Program::is_source_from_project_reference` | `rotor_tsrs::compiler::Program::is_source_from_project_reference` |
| `compiler` | method | `Program.Options` | 44 (compile,transformer) | `Program::options` | `rotor_tsrs::compiler::Program::options` |
| `compiler` | method | `Program.ResolveModuleName` | 1 (transformer) | `Program::resolve_module_name` (0004 patch) | `rotor_tsrs::compiler::Program::resolve_module_name` |
| `compiler` | method | `Program.SingleThreaded` | 7 (compile,flamework) | `Program::single_threaded` | `rotor_tsrs::compiler::Program::single_threaded` |
| `compiler` | method | `Program.SourceFiles` | 19 (compile) | `Program::source_files` | `rotor_tsrs::compiler::Program::source_files` |
| `compiler` | method | `Program.UpdateProgramFilesWithCheckers` | 2 (compile) | Program::update_program_with_checkers (one file per call; patch 0003 adds the checker count) | `rotor_tsrs::project::update_with_overlays` |
| `compiler` | method | `Program.UseCaseSensitiveFileNames` | 3 (compile,flamework,transformer) | `Program::use_case_sensitive_file_names` | `rotor_tsrs::compiler::Program::use_case_sensitive_file_names` |
| `compiler` | type | `CompilerHost` | 8 (compile) | `CompilerHost` | `rotor_tsrs::compiler::CompilerHost` |
| `compiler` | type | `EmitOptions` | 1 (compile) | `EmitOptions` | `rotor_tsrs::compiler::EmitOptions` |
| `compiler` | type | `Program` | 78 (compile,flamework,jecs,transformer) | `Program` | `rotor_tsrs::compiler::Program` |
| `compiler` | type | `ProgramOptions` | 1 (compile) | `ProgramOptions` | `rotor_tsrs::compiler::ProgramOptions` |
| `compiler` | type | `WriteFileData` | 2 (compile) | `WriteFileData` | `rotor_tsrs::compiler::WriteFileData` |
| `core` | const | `JsxEmitNone` | 1 (compile) | `JsxEmit::NONE` | `rotor_tsrs::tscore::JsxEmit::NONE` |
| `core` | const | `ModuleDetectionKindForce` | 1 (compile) | `ModuleDetectionKind::Force` | `rotor_tsrs::tscore::ModuleDetectionKind::Force` |
| `core` | const | `ModuleKindCommonJS` | 1 (compile) | `ModuleKind::COMMON_JS` | `rotor_tsrs::tscore::ModuleKind::COMMON_JS` |
| `core` | const | `ModuleKindPreserve` | 1 (compile) | `ModuleKind::PRESERVE` | `rotor_tsrs::tscore::ModuleKind::PRESERVE` |
| `core` | const | `ModuleResolutionKindBundler` | 1 (compile) | `ModuleResolutionKind::BUNDLER` | `rotor_tsrs::tscore::ModuleResolutionKind::BUNDLER` |
| `core` | const | `ResolutionModeCommonJS` | 1 (jecs) | `ResolutionMode::COMMON_JS` | `rotor_tsrs::tscore::ResolutionMode::COMMON_JS` |
| `core` | const | `ResolutionModeESM` | 1 (jecs) | `ResolutionMode::ESM` | `rotor_tsrs::tscore::ResolutionMode::ESM` |
| `core` | const | `ResolutionModeNone` | 4 (compile,jecs,transformer) | `ResolutionMode::NONE` | `rotor_tsrs::tscore::ResolutionMode::NONE` |
| `core` | const | `ScriptKindTS` | 2 (compile) | `ScriptKind::TS` | `rotor_tsrs::tscore::ScriptKind::TS` |
| `core` | const | `TSFalse` | 1 (compile) | `Tristate::False` | `rotor_tsrs::Tristate::False` |
| `core` | const | `TSTrue` | 4 (compile) | `Tristate::True` | `rotor_tsrs::Tristate::True` |
| `core` | const | `TSUnknown` | 1 (compile) | `Tristate::Unknown` | `rotor_tsrs::Tristate::Unknown` |
| `core` | field | `CompilerOptions.AllowJs` | 1 (compile) | `CompilerOptions::allow_js` | `rotor_tsrs::tscore::CompilerOptions::allow_js` |
| `core` | field | `CompilerOptions.AllowSyntheticDefaultImports` | 1 (compile) | `CompilerOptions::allow_synthetic_default_imports` | `rotor_tsrs::tscore::CompilerOptions::allow_synthetic_default_imports` |
| `core` | field | `CompilerOptions.BaseUrl` | 2 (compile) | `CompilerOptions::base_url` | `rotor_tsrs::tscore::CompilerOptions::base_url` |
| `core` | field | `CompilerOptions.Checkers` | 3 (compile) | `CompilerOptions::checkers` | `rotor_tsrs::tscore::CompilerOptions::checkers` |
| `core` | field | `CompilerOptions.Composite` | 1 (compile) | `CompilerOptions::composite` | `rotor_tsrs::tscore::CompilerOptions::composite` |
| `core` | field | `CompilerOptions.ConfigFilePath` | 21 (compile) | `CompilerOptions::config_file_path` | `rotor_tsrs::tscore::CompilerOptions::config_file_path` |
| `core` | field | `CompilerOptions.CustomConditions` | 2 (compile) | `CompilerOptions::custom_conditions` | `rotor_tsrs::tscore::CompilerOptions::custom_conditions` |
| `core` | field | `CompilerOptions.Declaration` | 4 (compile) | `CompilerOptions::declaration` | `rotor_tsrs::tscore::CompilerOptions::declaration` |
| `core` | field | `CompilerOptions.DeclarationDir` | 6 (compile) | `CompilerOptions::declaration_dir` | `rotor_tsrs::tscore::CompilerOptions::declaration_dir` |
| `core` | field | `CompilerOptions.GenerateCpuProfile` | 1 (compile) | `CompilerOptions::generate_cpu_profile` | `rotor_tsrs::tscore::CompilerOptions::generate_cpu_profile` |
| `core` | field | `CompilerOptions.GenerateTrace` | 1 (compile) | `CompilerOptions::generate_trace` | `rotor_tsrs::tscore::CompilerOptions::generate_trace` |
| `core` | field | `CompilerOptions.Jsx` | 1 (compile) | `CompilerOptions::jsx` | `rotor_tsrs::tscore::CompilerOptions::jsx` |
| `core` | field | `CompilerOptions.Module` | 2 (compile) | `CompilerOptions::module` | `rotor_tsrs::tscore::CompilerOptions::module` |
| `core` | field | `CompilerOptions.ModuleDetection` | 1 (compile) | `CompilerOptions::module_detection` | `rotor_tsrs::tscore::CompilerOptions::module_detection` |
| `core` | field | `CompilerOptions.NoEmit` | 1 (compile) | `CompilerOptions::no_emit` | `rotor_tsrs::tscore::CompilerOptions::no_emit` |
| `core` | field | `CompilerOptions.NoLib` | 1 (compile) | `CompilerOptions::no_lib` | `rotor_tsrs::tscore::CompilerOptions::no_lib` |
| `core` | field | `CompilerOptions.OutDir` | 13 (compile) | `CompilerOptions::out_dir` | `rotor_tsrs::tscore::CompilerOptions::out_dir` |
| `core` | field | `CompilerOptions.OutFile` | 1 (compile) | `CompilerOptions::out_file` | `rotor_tsrs::tscore::CompilerOptions::out_file` |
| `core` | field | `CompilerOptions.Paths` | 7 (compile,transformer) | `CompilerOptions::paths` | `rotor_tsrs::tscore::CompilerOptions::paths` |
| `core` | field | `CompilerOptions.PathsBasePath` | 1 (compile) | `CompilerOptions::paths_base_path` | `rotor_tsrs::tscore::CompilerOptions::paths_base_path` |
| `core` | field | `CompilerOptions.PprofDir` | 1 (compile) | `CompilerOptions::pprof_dir` | `rotor_tsrs::tscore::CompilerOptions::pprof_dir` |
| `core` | field | `CompilerOptions.PreserveConstEnums` | 1 (transformer) | `CompilerOptions::preserve_const_enums` | `rotor_tsrs::tscore::CompilerOptions::preserve_const_enums` |
| `core` | field | `CompilerOptions.PreserveSymlinks` | 1 (compile) | `CompilerOptions::preserve_symlinks` | `rotor_tsrs::tscore::CompilerOptions::preserve_symlinks` |
| `core` | field | `CompilerOptions.Project` | 1 (compile) | `CompilerOptions::project` | `rotor_tsrs::tscore::CompilerOptions::project` |
| `core` | field | `CompilerOptions.RemoveComments` | 1 (transformer) | `CompilerOptions::remove_comments` | `rotor_tsrs::tscore::CompilerOptions::remove_comments` |
| `core` | field | `CompilerOptions.ResolveJsonModule` | 1 (compile) | `CompilerOptions::resolve_json_module` | `rotor_tsrs::tscore::CompilerOptions::resolve_json_module` |
| `core` | field | `CompilerOptions.RootDir` | 8 (compile) | `CompilerOptions::root_dir` | `rotor_tsrs::tscore::CompilerOptions::root_dir` |
| `core` | field | `CompilerOptions.RootDirs` | 10 (compile) | `CompilerOptions::root_dirs` | `rotor_tsrs::tscore::CompilerOptions::root_dirs` |
| `core` | field | `CompilerOptions.SingleThreaded` | 4 (compile) | `CompilerOptions::single_threaded` | `rotor_tsrs::tscore::CompilerOptions::single_threaded` |
| `core` | field | `CompilerOptions.SourceMap` | 4 (compile,transformer) | `CompilerOptions::source_map` | `rotor_tsrs::tscore::CompilerOptions::source_map` |
| `core` | field | `CompilerOptions.Strict` | 1 (compile) | `CompilerOptions::strict` | `rotor_tsrs::tscore::CompilerOptions::strict` |
| `core` | field | `CompilerOptions.TsBuildInfoFile` | 5 (compile) | `CompilerOptions::ts_build_info_file` | `rotor_tsrs::tscore::CompilerOptions::ts_build_info_file` |
| `core` | field | `CompilerOptions.TypeRoots` | 11 (compile) | `CompilerOptions::type_roots` | `rotor_tsrs::tscore::CompilerOptions::type_roots` |
| `core` | field | `CompilerOptions.Types` | 2 (compile) | `CompilerOptions::types` | `rotor_tsrs::tscore::CompilerOptions::types` |
| `core` | field | `CompilerOptions.VerbatimModuleSyntax` | 2 (transformer) | `CompilerOptions::verbatim_module_syntax` | `rotor_tsrs::tscore::CompilerOptions::verbatim_module_syntax` |
| `core` | field | `ParsedOptions.FileNames` | 6 (compile) | `tsrs_tsoptions::ParsedOptions::file_names` | `rotor_tsrs::tsoptions::ParsedOptions::file_names` |
| `core` | func | `GetScriptKindFromFileName` | 4 (compile) | `get_script_kind_from_file_name` | `rotor_tsrs::tscore::get_script_kind_from_file_name` |
| `core` | func | `NewTextRange` | 8 (flamework,jecs,lsp,transformer) | `TextRange::new` | `rotor_tsrs::tscore::TextRange::new` |
| `core` | func | `NewWorkGroup` | 6 (compile) | `new_work_group` | `rotor_tsrs::tscore::new_work_group` |
| `core` | func | `UndefinedTextRange` | 1 (flamework) | `TextRange::synthetic` | `rotor_tsrs::tscore::TextRange::synthetic` |
| `core` | func | `Version` | 3 (compile) | `version` | `rotor_tsrs::tscore::version` |
| `core` | method | `CompilerOptions.Clone` | 1 (compile) | Clone for CompilerOptions | `rotor_tsrs::CompilerOptions::clone` |
| `core` | method | `CompilerOptions.GetEmitDeclarations` | 11 (compile) | `CompilerOptions::get_emit_declarations` | `rotor_tsrs::tscore::CompilerOptions::get_emit_declarations` |
| `core` | method | `CompilerOptions.GetModuleResolutionKind` | 1 (compile) | `CompilerOptions::get_module_resolution_kind` | `rotor_tsrs::tscore::CompilerOptions::get_module_resolution_kind` |
| `core` | method | `CompilerOptions.GetPathsBasePath` | 2 (compile,transformer) | `CompilerOptions::get_paths_base_path` | `rotor_tsrs::tscore::CompilerOptions::get_paths_base_path` |
| `core` | method | `CompilerOptions.IsIncremental` | 4 (compile) | `CompilerOptions::is_incremental` | `rotor_tsrs::tscore::CompilerOptions::is_incremental` |
| `core` | method | `TextRange.End` | 2 (transformer) | `TextRange::end` | `rotor_tsrs::tscore::TextRange::end` |
| `core` | method | `TextRange.Len` | 1 (compile) | `TextRange::len` | `rotor_tsrs::tscore::TextRange::len` |
| `core` | method | `TextRange.Pos` | 3 (compile,transformer) | `TextRange::pos` | `rotor_tsrs::tscore::TextRange::pos` |
| `core` | method | `Tristate.IsFalse` | 1 (compile) | `Tristate::is_false` | `rotor_tsrs::tscore::Tristate::is_false` |
| `core` | method | `Tristate.IsTrue` | 17 (compile,transformer) | `Tristate::is_true` | `rotor_tsrs::tscore::Tristate::is_true` |
| `core` | method | `WorkGroup.Queue` | 6 (compile) | `WorkGroup::queue` | `rotor_tsrs::tscore::WorkGroup::queue` |
| `core` | method | `WorkGroup.RunAndWait` | 6 (compile) | `WorkGroup::run_and_wait` | `rotor_tsrs::tscore::WorkGroup::run_and_wait` |
| `core` | type | `CompilerOptions` | 13 (compile) | `CompilerOptions` | `rotor_tsrs::tscore::CompilerOptions` |
| `core` | type | `ResolutionMode` | 3 (compile,jecs) | `ResolutionMode` | `rotor_tsrs::tscore::ResolutionMode` |
| `core` | type | `TextRange` | 5 (flamework,jecs) | `TextRange` | `rotor_tsrs::tscore::TextRange` |
| `diagnostics` | const | `CategoryError` | 5 (compile,flamework) | `Category::Error` | `rotor_tsrs::diagnostics::Category::Error` |
| `diagnostics` | const | `CategoryMessage` | 1 (flamework) | `Category::Message` | `rotor_tsrs::diagnostics::Category::Message` |
| `diagnostics` | const | `CategoryWarning` | 1 (flamework) | `Category::Warning` | `rotor_tsrs::diagnostics::Category::Warning` |
| `diagnostics` | type | `Category` | 1 (flamework) | `Category` | `rotor_tsrs::diagnostics::Category` |
| `diagnostics` | var | `Decorators_are_not_valid_here` | 1 (flamework) | `DECORATORS_ARE_NOT_VALID_HERE` | `rotor_tsrs::diagnostics::DECORATORS_ARE_NOT_VALID_HERE` |
| `diagnostics` | var | `Referenced_project_0_may_not_disable_emit` | 1 (compile) | `REFERENCED_PROJECT_0_MAY_NOT_DISABLE_EMIT` | `rotor_tsrs::diagnostics::REFERENCED_PROJECT_0_MAY_NOT_DISABLE_EMIT` |
| `diagnostics` | var | `Referenced_project_0_must_have_setting_composite_Colon_true` | 1 (compile) | `REFERENCED_PROJECT_0_MUST_HAVE_SETTING_COMPOSITE_COLON_TRUE` | `rotor_tsrs::diagnostics::REFERENCED_PROJECT_0_MUST_HAVE_SETTING_COMPOSITE_COLON_TRUE` |
| `diagnostics` | var | `Type_0_does_not_satisfy_the_constraint_1` | 1 (flamework) | `TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1` | `rotor_tsrs::diagnostics::TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1` |
| `diagnostics` | var | `X_0_is_declared_here` | 1 (flamework) | `X_0_IS_DECLARED_HERE` | `rotor_tsrs::diagnostics::X_0_IS_DECLARED_HERE` |
| `jsnum` | type | `Number` | 3 (transformer) | `Number` | `rotor_tsrs::jsnum::Number` |
| `json` | func | `Deterministic` | 3 (compile) | — | missing: a Go encoder option; serde_json maps already serialize in key order |
| `json` | func | `Marshal` | 5 (compile,lsp) | `marshal` | `rotor_tsrs::tscore::json::marshal` |
| `json` | func | `Unmarshal` | 3 (lsp) | `unmarshal` | `rotor_tsrs::tscore::json::unmarshal` |
| `json` | type | `Value` | 5 (lsp) | `Value` | `rotor_tsrs::tscore::json::Value` |
| `jsonrpc` | const | `CodeInternalError` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | const | `CodeInvalidParams` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | const | `CodeInvalidRequest` | 3 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | const | `CodeMethodNotFound` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | field | `Message.ID` | 3 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | field | `Message.Method` | 5 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | field | `Message.Params` | 2 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | field | `ResponseError.Code` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | field | `ResponseError.Message` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | func | `NewReader` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | func | `NewWriter` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | method | `Message.IsNotification` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | method | `Message.IsRequest` | 3 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | method | `Message.IsResponse` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | method | `Reader.Read` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | method | `Writer.Write` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `ID` | 3 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `JSONRPCVersion` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `Message` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `Reader` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `ResponseError` | 2 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `jsonrpc` | type | `Writer` | 1 (lsp) | — | missing: tsrs mirrors no JSON-RPC crate; rotor_lsp (Stage 7) owns its transport |
| `ls/lsconv` | func | `ComputeLSPLineStarts` | 1 (lsp) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `ls/lsconv` | func | `FileNameToDocumentURI` | 1 (lsp) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `ls/lsconv` | func | `NewConverters` | 1 (lsp) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `ls/lsconv` | method | `Converters.ToLSPRange` | 1 (lsp) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `ls/lsconv` | type | `LSPLineMap` | 1 (lsp) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `ls/lsutil` | func | `GetLastToken` | 2 (transformer) | — | missing: tsrs mirrors no language-service crate; rotor_lsp (Stage 7) |
| `lsp/lsproto` | const | `DiagnosticSeverityError` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `DiagnosticSeverityWarning` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `ErrorCodeServerNotInitialized` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MessageTypeError` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MessageTypeWarning` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodCancelRequest` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodExit` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodInitialize` | 3 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodShutdown` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodTextDocumentDidChange` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodTextDocumentDidClose` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodTextDocumentDidOpen` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `MethodTextDocumentDidSave` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `PositionEncodingKindUTF16` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `PositionEncodingKindUTF8` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | const | `TextDocumentSyncKindFull` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `BooleanOrSaveOptions.Boolean` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ClientCapabilities.General` | 3 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `Diagnostic.Code` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `Diagnostic.Message` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `Diagnostic.Range` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `Diagnostic.Severity` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `Diagnostic.Source` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidChangeTextDocumentParams.ContentChanges` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidChangeTextDocumentParams.TextDocument` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidCloseTextDocumentParams.TextDocument` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidOpenTextDocumentParams.TextDocument` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidSaveTextDocumentParams.Text` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `DidSaveTextDocumentParams.TextDocument` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `GeneralClientCapabilities.PositionEncodings` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `InitializeParams.Capabilities` | 4 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `InitializeResult.Capabilities` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `InitializeResult.ServerInfo` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `IntegerOrString.String` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `PublishDiagnosticsParams.Diagnostics` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `PublishDiagnosticsParams.Uri` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `RequestMessage.ID` | 7 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `RequestMessage.Method` | 6 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `RequestMessage.Params` | 5 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ServerCapabilities.PositionEncoding` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ServerCapabilities.TextDocumentSync` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ServerInfo.Name` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ShowMessageParams.Message` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `ShowMessageParams.Type` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `StringOrMarkupContent.String` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentContentChangePartialOrWholeDocument.WholeDocument` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentContentChangeWholeDocument.Text` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentIdentifier.Uri` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentItem.Text` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentItem.Uri` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentSyncOptions.Change` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentSyncOptions.OpenClose` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentSyncOptions.Save` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `TextDocumentSyncOptionsOrKind.Options` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | field | `VersionedTextDocumentIdentifier.Uri` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | method | `DocumentUri.FileName` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | method | `Message.AsRequest` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | method | `NotificationInfo.NewNotificationMessage` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `BooleanOrSaveOptions` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `Diagnostic` | 8 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DiagnosticSeverity` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DidChangeTextDocumentParams` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DidCloseTextDocumentParams` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DidOpenTextDocumentParams` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DidSaveTextDocumentParams` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `DocumentUri` | 8 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `InitializeParams` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `InitializeResult` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `IntegerOrString` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `Message` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `MessageType` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `Method` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `PositionEncodingKind` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `PublishDiagnosticsParams` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `Range` | 2 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `RequestMessage` | 5 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `ServerCapabilities` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `ServerInfo` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `ShowMessageParams` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `StringOrMarkupContent` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `TextDocumentItem` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `TextDocumentSyncOptions` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | type | `TextDocumentSyncOptionsOrKind` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | var | `TextDocumentPublishDiagnosticsInfo` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `lsp/lsproto` | var | `WindowShowMessageInfo` | 1 (lsp) | — | missing: tsrs mirrors no LSP protocol crate; rotor_lsp (Stage 7) owns its protocol types |
| `module` | field | `ModeAwareCacheKey.Mode` | 2 (compile) | `ModeAwareCacheKey::mode` | `rotor_tsrs::module::ModeAwareCacheKey::mode` |
| `module` | field | `ModeAwareCacheKey.Name` | 3 (compile,jecs) | `ModeAwareCacheKey::name` | `rotor_tsrs::module::ModeAwareCacheKey::name` |
| `module` | field | `ResolvedModule.Extension` | 1 (compile) | `ResolvedModule::extension` | `rotor_tsrs::module::ResolvedModule::extension` |
| `module` | field | `ResolvedModule.IsExternalLibraryImport` | 1 (compile) | `ResolvedModule::is_external_library_import` | `rotor_tsrs::module::ResolvedModule::is_external_library_import` |
| `module` | field | `ResolvedModule.ResolvedFileName` | 7 (compile,flamework,jecs,transformer) | `ResolvedModule::resolved_file_name` | `rotor_tsrs::module::ResolvedModule::resolved_file_name` |
| `module` | field | `ResolvedTypeReferenceDirective.ResolvedFileName` | 2 (compile) | `ResolvedTypeReferenceDirective::resolved_file_name` | `rotor_tsrs::module::ResolvedTypeReferenceDirective::resolved_file_name` |
| `module` | func | `NewResolver` | 1 (compile) | `new_resolver` | `rotor_tsrs::module::new_resolver` |
| `module` | func | `ResolveConfig` | 1 (compile) | `resolve_config` | `rotor_tsrs::module::resolve_config` |
| `module` | method | `ResolvedModule.IsResolved` | 9 (compile,flamework,jecs,transformer) | `ResolvedModule::is_resolved` | `rotor_tsrs::module::ResolvedModule::is_resolved` |
| `module` | method | `Resolver.ResolveModuleName` | 1 (compile) | `Resolver::resolve_module_name` | `rotor_tsrs::module::Resolver::resolve_module_name` |
| `module` | type | `ModeAwareCache` | 1 (compile) | `ModeAwareCache` | `rotor_tsrs::module::ModeAwareCache` |
| `module` | type | `ModeAwareCacheKey` | 2 (compile) | `ModeAwareCacheKey` | `rotor_tsrs::module::ModeAwareCacheKey` |
| `module` | type | `ResolvedModule` | 2 (compile) | `ResolvedModule` | `rotor_tsrs::module::ResolvedModule` |
| `module` | type | `Resolver` | 1 (compile) | `Resolver` | `rotor_tsrs::module::Resolver` |
| `nodebuilder` | const | `FlagsUseFullyQualifiedType` | 1 (flamework) | `Flags::UseFullyQualifiedType` | `rotor_tsrs::checker::NodeBuilderFlags::UseFullyQualifiedType` |
| `nodebuilder` | const | `FlagsWriteClassExpressionAsTypeLiteral` | 1 (flamework) | `Flags::WriteClassExpressionAsTypeLiteral` | `rotor_tsrs::checker::NodeBuilderFlags::WriteClassExpressionAsTypeLiteral` |
| `outputpaths` | func | `GetBuildInfoFileName` | 2 (compile) | `get_build_info_file_name` | `rotor_tsrs::tsoptions::outputpaths::get_build_info_file_name` |
| `outputpaths` | func | `GetCommonSourceDirectory` | 1 (compile) | `get_common_source_directory` | `rotor_tsrs::tsoptions::outputpaths::get_common_source_directory` |
| `outputpaths` | func | `GetOutputDeclarationFileNameWorker` | 4 (compile) | `get_output_declaration_file_name_worker` | `rotor_tsrs::tsoptions::outputpaths::get_output_declaration_file_name_worker` |
| `outputpaths` | method | `OutputPathsHost.CommonSourceDirectory` | 1 (compile) | `OutputPathsHost::common_source_directory` | `rotor_tsrs::tsoptions::outputpaths::OutputPathsHost::common_source_directory` |
| `outputpaths` | type | `OutputPathsHost` | 1 (compile) | `OutputPathsHost` | `rotor_tsrs::tsoptions::outputpaths::OutputPathsHost` |
| `parser` | func | `ParseIsolatedEntityName` | 1 (transformer) | `parse_isolated_entity_name` | `rotor_tsrs::parser::parse_isolated_entity_name` |
| `parser` | func | `ParseSourceFile` | 6 (compile) | `parse_source_file` | `rotor_tsrs::parser::parse_source_file` |
| `printer` | field | `EmitContext.Factory` | 3 (flamework,jecs) | `EmitContext::factory` | `rotor_tsrs::printer::EmitContext::factory` |
| `printer` | field | `PrinterOptions.InlineSources` | 1 (compile) | `PrinterOptions::inline_sources` | `rotor_tsrs::printer::PrinterOptions::inline_sources` |
| `printer` | field | `PrinterOptions.SourceMap` | 1 (compile) | `PrinterOptions::source_map` | `rotor_tsrs::printer::PrinterOptions::source_map` |
| `printer` | func | `NewEmitContext` | 3 (flamework,jecs) | `new_emit_context` | `rotor_tsrs::printer::new_emit_context` |
| `printer` | func | `NewPrinter` | 1 (compile) | `new_printer` | `rotor_tsrs::printer::new_printer` |
| `printer` | func | `NewTextWriter` | 1 (compile) | `new_text_writer` | `rotor_tsrs::printer::new_text_writer` |
| `printer` | method | `EmitContext.AddSyntheticLeadingComment` | 1 (flamework) | `EmitContext::add_synthetic_leading_comment` | `rotor_tsrs::printer::EmitContext::add_synthetic_leading_comment` |
| `printer` | method | `EmitContext.NewNodeVisitor` | 3 (jecs) | `EmitContext::new_node_visitor` | `rotor_tsrs::printer::EmitContext::new_node_visitor` |
| `printer` | method | `NodeFactory.NewAssignmentExpression` | 4 (jecs) | `NodeFactory::new_assignment_expression` | `rotor_tsrs::printer::NodeFactory::new_assignment_expression` |
| `printer` | method | `NodeFactory.NewFalseExpression` | 1 (jecs) | `NodeFactory::new_false_expression` | `rotor_tsrs::printer::NodeFactory::new_false_expression` |
| `printer` | method | `NodeFactory.NewGeneratedNameForNode` | 1 (jecs) | `NodeFactory::new_generated_name_for_node` | `rotor_tsrs::printer::NodeFactory::new_generated_name_for_node` |
| `printer` | method | `NodeFactory.NewStrictEqualityExpression` | 1 (jecs) | NodeFactory::new_strict_equality_expression (0002 patch) | `rotor_tsrs::printer::NodeFactory::new_strict_equality_expression` |
| `printer` | method | `NodeFactory.NewStrictInequalityExpression` | 1 (jecs) | `NodeFactory::new_strict_inequality_expression` | `rotor_tsrs::printer::NodeFactory::new_strict_inequality_expression` |
| `printer` | method | `NodeFactory.NewTrueExpression` | 1 (jecs) | `NodeFactory::new_true_expression` | `rotor_tsrs::printer::NodeFactory::new_true_expression` |
| `printer` | method | `NodeFactory.NewUniqueName` | 12 (jecs) | `NodeFactory::new_unique_name` | `rotor_tsrs::printer::NodeFactory::new_unique_name` |
| `printer` | method | `Printer.Write` | 1 (compile) | `Printer::write` | `rotor_tsrs::printer::Printer::write` |
| `printer` | type | `EmitContext` | 8 (compile,flamework,jecs) | `EmitContext` | `rotor_tsrs::printer::EmitContext` |
| `printer` | type | `NodeFactory` | 1 (jecs) | `NodeFactory` | `rotor_tsrs::printer::NodeFactory` |
| `printer` | type | `PrintHandlers` | 1 (compile) | `PrintHandlers` | `rotor_tsrs::printer::PrintHandlers` |
| `printer` | type | `PrinterOptions` | 1 (compile) | `PrinterOptions` | `rotor_tsrs::printer::PrinterOptions` |
| `scanner` | func | `GetECMALineAndByteOffsetOfPosition` | 1 (transformer) | `get_ecma_line_and_byte_offset_of_position` | `rotor_tsrs::scanner::get_ecma_line_and_byte_offset_of_position` |
| `scanner` | func | `GetECMALineAndUTF16CharacterOfPosition` | 2 (flamework,transformer) | `get_ecma_line_and_utf16_character_of_position` | `rotor_tsrs::scanner::get_ecma_line_and_utf16_character_of_position` |
| `scanner` | func | `GetLeadingCommentRanges` | 2 (transformer) | `get_leading_comment_ranges` | `rotor_tsrs::scanner::get_leading_comment_ranges` |
| `scanner` | func | `GetSourceTextOfNodeFromSourceFile` | 1 (jecs) | `get_source_text_of_node_from_source_file` | `rotor_tsrs::scanner::get_source_text_of_node_from_source_file` |
| `scanner` | func | `GetTextOfNode` | 3 (flamework) | `get_text_of_node` | `rotor_tsrs::scanner::get_text_of_node` |
| `scanner` | func | `GetTextOfNodeFromSourceText` | 1 (transformer) | `get_text_of_node_from_source_text` | `rotor_tsrs::scanner::get_text_of_node_from_source_text` |
| `scanner` | func | `GetTokenPosOfNode` | 7 (compile,flamework,jecs,transformer) | `get_token_pos_of_node` | `rotor_tsrs::scanner::get_token_pos_of_node` |
| `scanner` | func | `IsIntrinsicJsxName` | 1 (transformer) | `is_intrinsic_jsx_name` | `rotor_tsrs::scanner::is_intrinsic_jsx_name` |
| `scanner` | func | `SkipTrivia` | 2 (compile,flamework) | `skip_trivia` | `rotor_tsrs::scanner::skip_trivia` |
| `sourcemap` | field | `Mapping.GeneratedCharacter` | 2 (compile) | `Mapping::generated_character` | `rotor_tsrs::sourcemap::Mapping::generated_character` |
| `sourcemap` | field | `Mapping.GeneratedLine` | 4 (compile) | `Mapping::generated_line` | `rotor_tsrs::sourcemap::Mapping::generated_line` |
| `sourcemap` | field | `Mapping.SourceCharacter` | 2 (compile) | `Mapping::source_character` | `rotor_tsrs::sourcemap::Mapping::source_character` |
| `sourcemap` | field | `Mapping.SourceLine` | 4 (compile) | `Mapping::source_line` | `rotor_tsrs::sourcemap::Mapping::source_line` |
| `sourcemap` | field | `RawSourceMap.Mappings` | 1 (compile) | `RawSourceMap::mappings` | `rotor_tsrs::sourcemap::RawSourceMap::mappings` |
| `sourcemap` | field | `RawSourceMap.Version` | 1 (compile) | `RawSourceMap::version` | `rotor_tsrs::sourcemap::RawSourceMap::version` |
| `sourcemap` | func | `DecodeMappings` | 2 (compile) | `decode_mappings` | `rotor_tsrs::sourcemap::decode_mappings` |
| `sourcemap` | func | `NewGenerator` | 1 (compile) | `new_generator` | `rotor_tsrs::sourcemap::new_generator` |
| `sourcemap` | method | `Generator.RawSourceMap` | 1 (compile) | `Generator::raw_source_map` | `rotor_tsrs::sourcemap::Generator::raw_source_map` |
| `sourcemap` | method | `Mapping.IsSourceMapping` | 2 (compile) | `Mapping::is_source_mapping` | `rotor_tsrs::sourcemap::Mapping::is_source_mapping` |
| `sourcemap` | method | `MappingsDecoder.Error` | 2 (compile) | `MappingsDecoder::error` | `rotor_tsrs::sourcemap::MappingsDecoder::error` |
| `sourcemap` | method | `MappingsDecoder.Values` | 2 (compile) | `MappingsDecoder::values` | `rotor_tsrs::sourcemap::MappingsDecoder::values` |
| `stringutil` | func | `IsDigit` | 1 (transformer) | `is_digit` | `rotor_tsrs::tscore::stringutil::is_digit` |
| `stringutil` | func | `IsHexDigit` | 1 (transformer) | `is_hex_digit` | `rotor_tsrs::tscore::stringutil::is_hex_digit` |
| `stringutil` | func | `IsLineBreak` | 1 (transformer) | `is_line_break` | `rotor_tsrs::tscore::stringutil::is_line_break` |
| `stringutil` | func | `IsWhiteSpaceSingleLine` | 1 (transformer) | `is_white_space_single_line` | `rotor_tsrs::tscore::stringutil::is_white_space_single_line` |
| `symlinks` | method | `KnownSymlinks.DirectoriesByRealpath` | 2 (compile,transformer) | `KnownSymlinks::directories_by_realpath` | `rotor_tsrs::module::symlinks::KnownSymlinks::directories_by_realpath` |
| `symlinks` | method | `KnownSymlinks.FilesByRealpath` | 1 (compile) | `KnownSymlinks::files_by_realpath` | `rotor_tsrs::module::symlinks::KnownSymlinks::files_by_realpath` |
| `tsoptions` | field | `ParsedCommandLine.ParsedConfig` | 7 (compile) | `ParsedCommandLine::parsed_config` | `rotor_tsrs::tsoptions::ParsedCommandLine::parsed_config` |
| `tsoptions` | func | `GetParsedCommandLineOfConfigFile` | 1 (compile) | `get_parsed_command_line_of_config_file` | `rotor_tsrs::project::load_project` |
| `tsoptions` | method | `ParsedCommandLine.CommonSourceDirectory` | 1 (compile) | `ParsedCommandLine::common_source_directory` | `rotor_tsrs::tsoptions::ParsedCommandLine::common_source_directory` |
| `tsoptions` | method | `ParsedCommandLine.CompilerOptions` | 6 (compile) | `ParsedCommandLine::compiler_options` | `rotor_tsrs::tsoptions::ParsedCommandLine::compiler_options` |
| `tsoptions` | method | `ParsedCommandLine.ConfigName` | 6 (compile) | `ParsedCommandLine::config_name` | `rotor_tsrs::tsoptions::ParsedCommandLine::config_name` |
| `tsoptions` | method | `ParsedCommandLine.ExtendedSourceFiles` | 2 (compile) | `ParsedCommandLine::extended_source_files` | `rotor_tsrs::tsoptions::ParsedCommandLine::extended_source_files` |
| `tsoptions` | method | `ParsedCommandLine.FileNames` | 2 (compile) | `ParsedCommandLine::file_names` | `rotor_tsrs::tsoptions::ParsedCommandLine::file_names` |
| `tsoptions` | method | `ParsedCommandLine.ResolvedProjectReferencePaths` | 1 (compile) | `ParsedCommandLine::resolved_project_reference_paths` | `rotor_tsrs::tsoptions::ParsedCommandLine::resolved_project_reference_paths` |
| `tsoptions` | type | `ParsedCommandLine` | 9 (compile) | `ParsedCommandLine` | `rotor_tsrs::tsoptions::ParsedCommandLine` |
| `tspath` | const | `ExtensionDts` | 2 (compile) | `EXTENSION_DTS` | `rotor_tsrs::tspath::EXTENSION_DTS` |
| `tspath` | const | `ExtensionJs` | 1 (compile) | `EXTENSION_JS` | `rotor_tsrs::tspath::EXTENSION_JS` |
| `tspath` | const | `ExtensionJson` | 2 (compile) | `EXTENSION_JSON` | `rotor_tsrs::tspath::EXTENSION_JSON` |
| `tspath` | const | `ExtensionJsx` | 1 (compile) | `EXTENSION_JSX` | `rotor_tsrs::tspath::EXTENSION_JSX` |
| `tspath` | const | `ExtensionTs` | 1 (compile) | `EXTENSION_TS` | `rotor_tsrs::tspath::EXTENSION_TS` |
| `tspath` | const | `ExtensionTsx` | 1 (compile) | `EXTENSION_TSX` | `rotor_tsrs::tspath::EXTENSION_TSX` |
| `tspath` | field | `ComparePathsOptions.CurrentDirectory` | 3 (compile) | `ComparePathsOptions::current_directory` | `rotor_tsrs::tspath::ComparePathsOptions::current_directory` |
| `tspath` | field | `ComparePathsOptions.UseCaseSensitiveFileNames` | 6 (compile,flamework) | `ComparePathsOptions::use_case_sensitive_file_names` | `rotor_tsrs::tspath::ComparePathsOptions::use_case_sensitive_file_names` |
| `tspath` | func | `CombinePaths` | 2 (compile,transformer) | `combine_paths` | `rotor_tsrs::tspath::combine_paths` |
| `tspath` | func | `ContainsPath` | 1 (compile) | `contains_path` | `rotor_tsrs::tspath::contains_path` |
| `tspath` | func | `EnsurePathIsNonModuleName` | 1 (flamework) | `ensure_path_is_non_module_name` | `rotor_tsrs::tspath::ensure_path_is_non_module_name` |
| `tspath` | func | `EnsureTrailingDirectorySeparator` | 1 (transformer) | `ensure_trailing_directory_separator` | `rotor_tsrs::tspath::ensure_trailing_directory_separator` |
| `tspath` | func | `FileExtensionIs` | 1 (compile) | `file_extension_is` | `rotor_tsrs::tspath::file_extension_is` |
| `tspath` | func | `GetBaseFileName` | 3 (compile,transformer) | `get_base_file_name` | `rotor_tsrs::tspath::get_base_file_name` |
| `tspath` | func | `GetDirectoryPath` | 9 (compile,flamework,transformer) | `get_directory_path` | `rotor_tsrs::tspath::get_directory_path` |
| `tspath` | func | `GetRelativePathFromDirectory` | 2 (compile,flamework) | `get_relative_path_from_directory` | `rotor_tsrs::tspath::get_relative_path_from_directory` |
| `tspath` | func | `GetRelativePathToDirectoryOrUrl` | 1 (compile) | `get_relative_path_to_directory_or_url` | `rotor_tsrs::tspath::get_relative_path_to_directory_or_url` |
| `tspath` | func | `GetRootLength` | 2 (compile) | `get_root_length` | `rotor_tsrs::tspath::get_root_length` |
| `tspath` | func | `HasJSONFileExtension` | 2 (compile) | `has_json_file_extension` | `rotor_tsrs::tspath::has_json_file_extension` |
| `tspath` | func | `IsDeclarationFileName` | 10 (compile,transformer) | `is_declaration_file_name` | `rotor_tsrs::tspath::is_declaration_file_name` |
| `tspath` | func | `IsRootedDiskPath` | 1 (compile) | `is_rooted_disk_path` | `rotor_tsrs::tspath::is_rooted_disk_path` |
| `tspath` | func | `NormalizePath` | 6 (compile) | `normalize_path` | `rotor_tsrs::tspath::normalize_path` |
| `tspath` | func | `NormalizeSlashes` | 1 (compile) | `normalize_slashes` | `rotor_tsrs::tspath::normalize_slashes` |
| `tspath` | func | `RemoveFileExtension` | 1 (flamework) | `remove_file_extension` | `rotor_tsrs::tspath::remove_file_extension` |
| `tspath` | func | `ResolvePath` | 3 (transformer) | `resolve_path` | `rotor_tsrs::tspath::resolve_path` |
| `tspath` | func | `ToPath` | 5 (compile,transformer) | `to_path` | `rotor_tsrs::tspath::to_path` |
| `tspath` | type | `ComparePathsOptions` | 8 (compile,flamework) | `ComparePathsOptions` | `rotor_tsrs::tspath::ComparePathsOptions` |
| `tspath` | type | `Path` | 5 (compile) | `Path` | `rotor_tsrs::tspath::Path` |
| `vfs` | method | `FS.DirectoryExists` | 2 (transformer) | `FS::directory_exists` | `rotor_tsrs::vfs::FS::directory_exists` |
| `vfs` | method | `FS.FileExists` | 4 (compile,transformer) | `FS::file_exists` | `rotor_tsrs::vfs::FS::file_exists` |
| `vfs` | method | `FS.ReadFile` | 7 (compile) | `FS::read_file` | `rotor_tsrs::vfs::FS::read_file` |
| `vfs` | method | `FS.Realpath` | 7 (compile,transformer) | `FS::realpath` | `rotor_tsrs::vfs::FS::realpath` |
| `vfs` | method | `FS.UseCaseSensitiveFileNames` | 41 (compile) | `FS::use_case_sensitive_file_names` | `rotor_tsrs::vfs::FS::use_case_sensitive_file_names` |
| `vfs` | type | `FS` | 19 (compile) | `FS` | `rotor_tsrs::vfs::FS` |
| `vfs/cachedvfs` | func | `From` | 3 (compile) | `from` | `rotor_tsrs::vfs::cachedvfs::from` |
| `vfs/cachedvfs` | method | `FS.FileExists` | 1 (compile) | `FS::file_exists` | `rotor_tsrs::vfs::cachedvfs::FS::file_exists` |
| `vfs/cachedvfs` | method | `FS.ReadFile` | 1 (compile) | `FS::read_file` | `rotor_tsrs::vfs::cachedvfs::FS::read_file` |
| `vfs/cachedvfs` | method | `FS.UseCaseSensitiveFileNames` | 1 (compile) | `FS::use_case_sensitive_file_names` | `rotor_tsrs::vfs::cachedvfs::FS::use_case_sensitive_file_names` |
| `vfs/osvfs` | func | `FS` | 40 (compile,transformer) | `fs` | `rotor_tsrs::vfs::osvfs::fs` |
| `vfs/wrapvfs` | field | `Replacements.FileExists` | 4 (compile) | `Replacements::file_exists` | `rotor_tsrs::vfs::wrapvfs::Replacements::file_exists` |
| `vfs/wrapvfs` | field | `Replacements.ReadFile` | 5 (compile) | `Replacements::read_file` | `rotor_tsrs::vfs::wrapvfs::Replacements::read_file` |
| `vfs/wrapvfs` | func | `Wrap` | 5 (compile) | `wrap` | `rotor_tsrs::vfs::wrapvfs::wrap` |
| `vfs/wrapvfs` | type | `Replacements` | 5 (compile) | `Replacements` | `rotor_tsrs::vfs::wrapvfs::Replacements` |
