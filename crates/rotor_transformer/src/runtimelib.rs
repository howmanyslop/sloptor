//! `runtimelib.go`: the `local TS = ...` runtime library import.

use rotor_rojo::{RelativeRbxPathSegment, relative};
use rotor_support::gopath;

use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::prelude::*;
use crate::state::ProjectType;

/// `PARENT_FIELD` (`Shared/constants.ts`): the instance property a parent segment renders as.
const PARENT_FIELD: &str = "Parent";

/// `createGetService`: `game:GetService("<serviceName>")`.
pub fn create_get_service(ast: &mut Ast, service_name: &str) -> NodeId {
	let game = ast.global_id("game");
	let name = ast.str(service_name);
	let args = ast.list_from([name]);
	ast.method_call("GetService", game, args)
}

/// `propertyAccessExpressionChain`: `["a", "b"]` over `expression` gives `expression.a.b`.
pub fn property_access_expression_chain(ast: &mut Ast, expression: NodeId, names: &[String]) -> NodeId {
	let mut accumulated = convert_to_indexable_expression(ast, expression);
	for name in names {
		accumulated = ast.property_access(accumulated, name.as_str());
	}
	accumulated
}

impl State<'_> {
	/// `CreateRuntimeLibImport` (`TransformState.ts:202-265`).
	///
	/// - Game: `local TS = require(game:GetService("<p0>"):WaitForChild("<p1>"):...)`.
	/// - Model: `local TS = require(script.<chain>)`, the chain being the Rojo-relative path
	///   with parent segments as `.Parent`. A file the Rojo config does not cover raises
	///   `noRojoData` and falls back to `local TS`.
	/// - Package: `local TS = _G[script]`.
	///
	/// The branch is keyed on the runtime library path, exactly like upstream's
	/// `if (this.runtimeLibRbxPath)`.
	pub fn create_runtime_lib_import(&mut self, node: P<SourceFile>) -> NodeId {
		let ts_id = self.ast.global_id("TS");

		if let Some(rojo) = self.rojo
			&& let Some(runtime_lib_rbx_path) = rojo.runtime_lib_rbx_path.as_ref().filter(|path| !path.is_empty())
		{
			if self.project_type == Some(ProjectType::Game) {
				let mut expression = create_get_service(&mut self.ast, &runtime_lib_rbx_path[0]);
				for part in &runtime_lib_rbx_path[1..] {
					let part = self.ast.str(part.as_str());
					let args = self.ast.list_from([part]);
					expression = self.ast.method_call("WaitForChild", expression, args);
				}
				let require = self.ast.global_id("require");
				let args = self.ast.list_from([expression]);
				let call = self.ast.call(require, args);
				return self.ast.variable_declaration(ts_id, Some(call.into()));
			}

			let source_out_path = rojo.path_translator.get_output_path(node.file_name());
			let Some(rbx_path) = rojo.resolver.get_rbx_path_from_file_path(&source_out_path) else {
				let rel_path = gopath::rel(&rojo.project_path, &source_out_path).unwrap_or(source_out_path);
				self.diags
					.add(diagnostics::no_rojo_data(node.as_node(), &rel_path, false));
				return self.ast.variable_declaration(ts_id, None);
			};

			let names: Vec<String> = relative(&rbx_path, runtime_lib_rbx_path)
				.into_iter()
				.map(|segment| match segment {
					RelativeRbxPathSegment::Parent => PARENT_FIELD.to_owned(),
					RelativeRbxPathSegment::Name(name) => name,
				})
				.collect();
			let script = self.ast.global_id("script");
			let chain = property_access_expression_chain(&mut self.ast, script, &names);
			let require = self.ast.global_id("require");
			let args = self.ast.list_from([chain]);
			let call = self.ast.call(require, args);
			return self.ast.variable_declaration(ts_id, Some(call.into()));
		}

		// Package (TransformState.ts:254-264): local TS = _G[script]
		let global = self.ast.global_id("_G");
		let script = self.ast.global_id("script");
		let index = self.ast.computed_index(global, script);
		self.ast.variable_declaration(ts_id, Some(index.into()))
	}
}
