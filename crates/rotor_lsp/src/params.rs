//! Which request params `tsgo/lsp/lsproto` fails to decode (`unmarshalParams`), so a request
//! for a known method with such params is answered with `CodeInvalidParams`, as Go's server does.
//!
//! Maintained by hand, from the table `tsgo/lsp/lsproto/lsp_generated.go` had in the retired Go
//! implementation (no generator remains). Only the params' top level is checked:
//! an object, its required properties present and no property null that Go rejects as null.
//! A params object malformed deeper than that is answered as an unknown method.

use serde_json::Value;

/// How a method's params decode.
enum Shape {
	/// `unmarshalEmpty`: any params fail.
	Empty,
	/// `unmarshalPtrTo` of a struct with no decoder of its own: an object, or null.
	Plain,
	/// `unmarshalPtrTo`: an object with these required properties, and these that may not be null.
	Object {
		required: &'static [&'static str],
		not_null: &'static [&'static str],
	},
}

const METHODS: &[(&str, Shape)] = &[
	// CancelParams
	(
		"$/cancelRequest",
		Shape::Object {
			required: &["id"],
			not_null: &[],
		},
	),
	// LogTraceParams
	(
		"$/logTrace",
		Shape::Object {
			required: &["message"],
			not_null: &["verbose"],
		},
	),
	// ProgressParams
	(
		"$/progress",
		Shape::Object {
			required: &["token", "value"],
			not_null: &[],
		},
	),
	// SetTraceParams
	(
		"$/setTrace",
		Shape::Object {
			required: &["value"],
			not_null: &[],
		},
	),
	// CallHierarchyIncomingCallsParams
	(
		"callHierarchy/incomingCalls",
		Shape::Object {
			required: &["item"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// CallHierarchyOutgoingCallsParams
	(
		"callHierarchy/outgoingCalls",
		Shape::Object {
			required: &["item"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// RegistrationParams
	(
		"client/registerCapability",
		Shape::Object {
			required: &["registrations"],
			not_null: &[],
		},
	),
	// UnregistrationParams
	(
		"client/unregisterCapability",
		Shape::Object {
			required: &["unregisterations"],
			not_null: &[],
		},
	),
	// CodeAction
	(
		"codeAction/resolve",
		Shape::Object {
			required: &["title"],
			not_null: &[
				"kind",
				"diagnostics",
				"isPreferred",
				"disabled",
				"edit",
				"command",
				"data",
				"tags",
			],
		},
	),
	// CodeLens
	(
		"codeLens/resolve",
		Shape::Object {
			required: &["range"],
			not_null: &["command", "data"],
		},
	),
	// CompletionItem
	(
		"completionItem/resolve",
		Shape::Object {
			required: &["label"],
			not_null: &[
				"labelDetails",
				"kind",
				"tags",
				"detail",
				"documentation",
				"deprecated",
				"preselect",
				"sortText",
				"filterText",
				"insertText",
				"insertTextFormat",
				"insertTextMode",
				"textEdit",
				"textEditText",
				"additionalTextEdits",
				"commitCharacters",
				"command",
				"data",
			],
		},
	),
	// InitializeAPISessionParams
	(
		"custom/initializeAPISession",
		Shape::Object {
			required: &[],
			not_null: &["pipe"],
		},
	),
	// ProjectInfoParams
	(
		"custom/projectInfo",
		Shape::Object {
			required: &["textDocument"],
			not_null: &[],
		},
	),
	("custom/runGC", Shape::Empty),
	// ProfileParams
	(
		"custom/saveAllocProfile",
		Shape::Object {
			required: &["dir"],
			not_null: &[],
		},
	),
	// ProfileParams
	(
		"custom/saveHeapProfile",
		Shape::Object {
			required: &["dir"],
			not_null: &[],
		},
	),
	// SetLogVerbosityParams
	(
		"custom/setLogVerbosity",
		Shape::Object {
			required: &["verbosity"],
			not_null: &[],
		},
	),
	// ProfileParams
	(
		"custom/startCPUProfile",
		Shape::Object {
			required: &["dir"],
			not_null: &[],
		},
	),
	("custom/stopCPUProfile", Shape::Empty),
	// MultiDocumentHighlightParams
	(
		"custom/textDocument/multiDocumentHighlight",
		Shape::Object {
			required: &["textDocument", "position", "filesToSearch"],
			not_null: &[],
		},
	),
	// TextDocumentPositionParams
	(
		"custom/textDocument/sourceDefinition",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &[],
		},
	),
	// DocumentLink
	(
		"documentLink/resolve",
		Shape::Object {
			required: &["range"],
			not_null: &["target", "tooltip", "data"],
		},
	),
	("exit", Shape::Empty),
	// InitializeParams
	(
		"initialize",
		Shape::Object {
			required: &["processId", "rootUri", "capabilities"],
			not_null: &[
				"workDoneToken",
				"clientInfo",
				"locale",
				"initializationOptions",
				"trace",
			],
		},
	),
	// InitializedParams: no decoder of its own
	("initialized", Shape::Plain),
	// InlayHint
	(
		"inlayHint/resolve",
		Shape::Object {
			required: &["position", "label"],
			not_null: &["kind", "textEdits", "tooltip", "paddingLeft", "paddingRight", "data"],
		},
	),
	("shutdown", Shape::Empty),
	// VSOnAutoInsertParams
	(
		"textDocument/_vs_onAutoInsert",
		Shape::Object {
			required: &["_vs_textDocument", "_vs_position", "_vs_ch"],
			not_null: &[],
		},
	),
	// ReferenceParams
	(
		"textDocument/_vs_references",
		Shape::Object {
			required: &["textDocument", "position", "context"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// CodeActionParams
	(
		"textDocument/codeAction",
		Shape::Object {
			required: &["textDocument", "range", "context"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// CodeLensParams
	(
		"textDocument/codeLens",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// ColorPresentationParams
	(
		"textDocument/colorPresentation",
		Shape::Object {
			required: &["textDocument", "color", "range"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// CompletionParams
	(
		"textDocument/completion",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken", "context"],
		},
	),
	// DeclarationParams
	(
		"textDocument/declaration",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DefinitionParams
	(
		"textDocument/definition",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentDiagnosticParams
	(
		"textDocument/diagnostic",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken", "identifier", "previousResultId"],
		},
	),
	// DidChangeTextDocumentParams
	(
		"textDocument/didChange",
		Shape::Object {
			required: &["textDocument", "contentChanges"],
			not_null: &[],
		},
	),
	// DidCloseTextDocumentParams
	(
		"textDocument/didClose",
		Shape::Object {
			required: &["textDocument"],
			not_null: &[],
		},
	),
	// DidOpenTextDocumentParams
	(
		"textDocument/didOpen",
		Shape::Object {
			required: &["textDocument"],
			not_null: &[],
		},
	),
	// DidSaveTextDocumentParams
	(
		"textDocument/didSave",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["text"],
		},
	),
	// DocumentColorParams
	(
		"textDocument/documentColor",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentHighlightParams
	(
		"textDocument/documentHighlight",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentLinkParams
	(
		"textDocument/documentLink",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentSymbolParams
	(
		"textDocument/documentSymbol",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// FoldingRangeParams
	(
		"textDocument/foldingRange",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentFormattingParams
	(
		"textDocument/formatting",
		Shape::Object {
			required: &["textDocument", "options"],
			not_null: &["workDoneToken"],
		},
	),
	// HoverParams
	(
		"textDocument/hover",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "verbosityLevel"],
		},
	),
	// ImplementationParams
	(
		"textDocument/implementation",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// InlayHintParams
	(
		"textDocument/inlayHint",
		Shape::Object {
			required: &["textDocument", "range"],
			not_null: &["workDoneToken"],
		},
	),
	// InlineCompletionParams
	(
		"textDocument/inlineCompletion",
		Shape::Object {
			required: &["textDocument", "position", "context"],
			not_null: &["workDoneToken"],
		},
	),
	// InlineValueParams
	(
		"textDocument/inlineValue",
		Shape::Object {
			required: &["textDocument", "range", "context"],
			not_null: &["workDoneToken"],
		},
	),
	// LinkedEditingRangeParams
	(
		"textDocument/linkedEditingRange",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken"],
		},
	),
	// MonikerParams
	(
		"textDocument/moniker",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// DocumentOnTypeFormattingParams
	(
		"textDocument/onTypeFormatting",
		Shape::Object {
			required: &["textDocument", "position", "ch", "options"],
			not_null: &[],
		},
	),
	// CallHierarchyPrepareParams
	(
		"textDocument/prepareCallHierarchy",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken"],
		},
	),
	// PrepareRenameParams
	(
		"textDocument/prepareRename",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken"],
		},
	),
	// TypeHierarchyPrepareParams
	(
		"textDocument/prepareTypeHierarchy",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken"],
		},
	),
	// PublishDiagnosticsParams
	(
		"textDocument/publishDiagnostics",
		Shape::Object {
			required: &["uri", "diagnostics"],
			not_null: &["version"],
		},
	),
	// DocumentRangeFormattingParams
	(
		"textDocument/rangeFormatting",
		Shape::Object {
			required: &["textDocument", "range", "options"],
			not_null: &["workDoneToken"],
		},
	),
	// DocumentRangesFormattingParams
	(
		"textDocument/rangesFormatting",
		Shape::Object {
			required: &["textDocument", "ranges", "options"],
			not_null: &["workDoneToken"],
		},
	),
	// ReferenceParams
	(
		"textDocument/references",
		Shape::Object {
			required: &["textDocument", "position", "context"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// RenameParams
	(
		"textDocument/rename",
		Shape::Object {
			required: &["textDocument", "position", "newName"],
			not_null: &["workDoneToken"],
		},
	),
	// SelectionRangeParams
	(
		"textDocument/selectionRange",
		Shape::Object {
			required: &["textDocument", "positions"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// SemanticTokensParams
	(
		"textDocument/semanticTokens/full",
		Shape::Object {
			required: &["textDocument"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// SemanticTokensDeltaParams
	(
		"textDocument/semanticTokens/full/delta",
		Shape::Object {
			required: &["textDocument", "previousResultId"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// SemanticTokensRangeParams
	(
		"textDocument/semanticTokens/range",
		Shape::Object {
			required: &["textDocument", "range"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// SignatureHelpParams
	(
		"textDocument/signatureHelp",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "context"],
		},
	),
	// TypeDefinitionParams
	(
		"textDocument/typeDefinition",
		Shape::Object {
			required: &["textDocument", "position"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// WillSaveTextDocumentParams
	(
		"textDocument/willSave",
		Shape::Object {
			required: &["textDocument", "reason"],
			not_null: &[],
		},
	),
	// WillSaveTextDocumentParams
	(
		"textDocument/willSaveWaitUntil",
		Shape::Object {
			required: &["textDocument", "reason"],
			not_null: &[],
		},
	),
	// TypeHierarchySubtypesParams
	(
		"typeHierarchy/subtypes",
		Shape::Object {
			required: &["item"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// TypeHierarchySupertypesParams
	(
		"typeHierarchy/supertypes",
		Shape::Object {
			required: &["item"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// LogMessageParams
	(
		"window/logMessage",
		Shape::Object {
			required: &["type", "message"],
			not_null: &[],
		},
	),
	// ShowDocumentParams
	(
		"window/showDocument",
		Shape::Object {
			required: &["uri"],
			not_null: &["external", "takeFocus", "selection"],
		},
	),
	// ShowMessageParams
	(
		"window/showMessage",
		Shape::Object {
			required: &["type", "message"],
			not_null: &[],
		},
	),
	// ShowMessageRequestParams
	(
		"window/showMessageRequest",
		Shape::Object {
			required: &["type", "message"],
			not_null: &["actions"],
		},
	),
	// WorkDoneProgressCancelParams
	(
		"window/workDoneProgress/cancel",
		Shape::Object {
			required: &["token"],
			not_null: &[],
		},
	),
	// WorkDoneProgressCreateParams
	(
		"window/workDoneProgress/create",
		Shape::Object {
			required: &["token"],
			not_null: &[],
		},
	),
	// ApplyWorkspaceEditParams
	(
		"workspace/applyEdit",
		Shape::Object {
			required: &["edit"],
			not_null: &["label", "metadata"],
		},
	),
	("workspace/codeLens/refresh", Shape::Empty),
	// ConfigurationParams
	(
		"workspace/configuration",
		Shape::Object {
			required: &["items"],
			not_null: &[],
		},
	),
	// WorkspaceDiagnosticParams
	(
		"workspace/diagnostic",
		Shape::Object {
			required: &["previousResultIds"],
			not_null: &["workDoneToken", "partialResultToken", "identifier"],
		},
	),
	("workspace/diagnostic/refresh", Shape::Empty),
	// DidChangeConfigurationParams
	(
		"workspace/didChangeConfiguration",
		Shape::Object {
			required: &["settings"],
			not_null: &[],
		},
	),
	// DidChangeWatchedFilesParams
	(
		"workspace/didChangeWatchedFiles",
		Shape::Object {
			required: &["changes"],
			not_null: &[],
		},
	),
	// DidChangeWorkspaceFoldersParams
	(
		"workspace/didChangeWorkspaceFolders",
		Shape::Object {
			required: &["event"],
			not_null: &[],
		},
	),
	// CreateFilesParams
	(
		"workspace/didCreateFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	// DeleteFilesParams
	(
		"workspace/didDeleteFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	// RenameFilesParams
	(
		"workspace/didRenameFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	// ExecuteCommandParams
	(
		"workspace/executeCommand",
		Shape::Object {
			required: &["command"],
			not_null: &["workDoneToken", "arguments"],
		},
	),
	("workspace/foldingRange/refresh", Shape::Empty),
	("workspace/inlayHint/refresh", Shape::Empty),
	("workspace/inlineValue/refresh", Shape::Empty),
	("workspace/semanticTokens/refresh", Shape::Empty),
	// WorkspaceSymbolParams
	(
		"workspace/symbol",
		Shape::Object {
			required: &["query"],
			not_null: &["workDoneToken", "partialResultToken"],
		},
	),
	// TextDocumentContentParams
	(
		"workspace/textDocumentContent",
		Shape::Object {
			required: &["uri"],
			not_null: &[],
		},
	),
	// TextDocumentContentRefreshParams
	(
		"workspace/textDocumentContent/refresh",
		Shape::Object {
			required: &["uri"],
			not_null: &[],
		},
	),
	// CreateFilesParams
	(
		"workspace/willCreateFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	// DeleteFilesParams
	(
		"workspace/willDeleteFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	// RenameFilesParams
	(
		"workspace/willRenameFiles",
		Shape::Object {
			required: &["files"],
			not_null: &[],
		},
	),
	("workspace/workspaceFolders", Shape::Empty),
	// WorkspaceSymbol
	(
		"workspaceSymbol/resolve",
		Shape::Object {
			required: &["name", "kind", "location"],
			not_null: &["tags", "containerName", "data"],
		},
	),
];

/// Whether Go's `unmarshalParams` rejects `params` (present in the message, `null` included)
/// for `method`. An unknown method takes any JSON.
#[must_use]
pub fn rejects(method: &str, params: &Value) -> bool {
	let Ok(index) = METHODS.binary_search_by(|(name, _)| (*name).cmp(method)) else {
		return false;
	};
	match &METHODS[index].1 {
		Shape::Empty => true,
		Shape::Plain => !(params.is_object() || params.is_null()),
		Shape::Object { required, not_null } => {
			let Some(object) = params.as_object() else {
				return true;
			};
			required.iter().any(|name| !object.contains_key(*name))
				|| not_null
					.iter()
					.any(|name| object.get(*name).is_some_and(Value::is_null))
		}
	}
}
