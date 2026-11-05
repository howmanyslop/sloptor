//! `runtimelib_test.go`

use rotor_rojo::{PathTranslator, RbxPath, RojoResolver};

use super::{render_list, with_test_state};
use crate::nodemodulespath::NodeModulesPathMapping;
use crate::runtimelib::property_access_expression_chain;
use crate::state::{ProjectType, RojoContext};

/// A Rojo context holding only the runtime library path (Go's `&RojoContext{RuntimeLibRbxPath:
/// ...}`); leaked so it outlives the state.
fn rojo_context(runtime_lib_rbx_path: Option<RbxPath>) -> &'static RojoContext {
	Box::leak(Box::new(RojoContext {
		resolver: RojoResolver::synthetic(""),
		path_translator: PathTranslator::new("", "", "", false, true),
		runtime_lib_rbx_path,
		project_path: String::new(),
		pkg_rojo_resolvers: Vec::new(),
		node_modules_path_mapping: NodeModulesPathMapping::default(),
		project_reference_paths: std::collections::HashMap::new(),
		node_modules_path: String::new(),
		type_roots: Vec::new(),
		use_case_sensitive_file_names: true,
	}))
}

#[test]
fn create_runtime_lib_import_game_wait_for_child_chain() {
	with_test_state(|s| {
		let path = ["ReplicatedStorage", "rbxts_include", "RuntimeLib"]
			.map(str::to_owned)
			.to_vec();
		s.rojo = Some(rojo_context(Some(path)));
		s.project_type = Some(ProjectType::Game);
		let file = s.source_file;
		let statement = s.create_runtime_lib_import(file);
		let list = s.ast.list_from([statement]);
		assert_eq!(
			render_list(s, list),
			"local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"rbxts_include\"):WaitForChild(\"RuntimeLib\"))\n"
		);
	});
}

#[test]
fn create_runtime_lib_import_package_g_script() {
	with_test_state(|s| {
		// no RuntimeLibRbxPath = upstream undefined
		s.rojo = Some(rojo_context(None));
		s.project_type = Some(ProjectType::Package);
		let file = s.source_file;
		let statement = s.create_runtime_lib_import(file);
		let list = s.ast.list_from([statement]);
		assert_eq!(render_list(s, list), "local TS = _G[script]\n");
	});
}

/// `propertyAccessExpressionChain` ports `util/expressionChain.ts`; the Model require chain builds
/// on it.
#[test]
fn property_access_expression_chain_renders() {
	with_test_state(|s| {
		let script = s.ast.global_id("script");
		let names = ["Parent", "include", "RuntimeLib"].map(str::to_owned);
		let expression = property_access_expression_chain(&mut s.ast, script, &names);
		let require = s.ast.global_id("require");
		let args = s.ast.list_from([expression]);
		let call = s.ast.call(require, args);
		let ts = s.ast.global_id("TS");
		let declaration = s.ast.variable_declaration(ts, Some(call.into()));
		let list = s.ast.list_from([declaration]);
		assert_eq!(
			render_list(s, list),
			"local TS = require(script.Parent.include.RuntimeLib)\n"
		);
	});
}
