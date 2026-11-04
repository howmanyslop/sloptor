//! `cmd/rotor/options.go`: rbxtsc's `ProjectOptions` merge (defaults < tsconfig `rbxts` key <
//! argv), `findTsConfigPath`, and the compile options a build runs with.

use rotor_compile::ProjectOptions;
use rotor_compile::rbxts_options::{RbxtsOptions, read_rbxts_options};
use rotor_support::gopath;

/// `partialProjectOptions`: a `Partial<ProjectOptions>`, where `None` is absent. It is both the
/// tsconfig `rbxts` layer and the argv layer, so an absent CLI boolean never clobbers the
/// tsconfig's value (upstream's "DO NOT PROVIDE DEFAULTS BELOW HERE").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartialOptions {
	pub include_path: Option<String>,
	pub rojo: Option<String>,
	pub type_name: Option<String>,
	pub watch: Option<bool>,
	pub use_polling: Option<bool>,
	pub verbose: Option<bool>,
	pub no_include: Option<bool>,
	pub log_truthy_changes: Option<bool>,
	pub write_only_changed: Option<bool>,
	pub write_transformed_files: Option<bool>,
	pub optimized_array_appends: Option<bool>,
	pub optimized_loops: Option<bool>,
	pub allow_comment_directives: Option<bool>,
	pub luau: Option<bool>,
	pub luau_types: Option<bool>,
}

impl PartialOptions {
	/// `rbxtsOptions`: the rbxts-key subset, the command-line layer a solution replays over each
	/// referenced project.
	#[must_use]
	pub fn rbxts_options(&self) -> RbxtsOptions {
		RbxtsOptions {
			allow_comment_directives: self.allow_comment_directives,
			include_path: self.include_path.clone(),
			log_truthy_changes: self.log_truthy_changes,
			luau: self.luau,
			luau_types: self.luau_types,
			no_include: self.no_include,
			optimized_array_appends: self.optimized_array_appends,
			optimized_loops: self.optimized_loops,
			rojo: self.rojo.clone(),
			project_type: self.type_name.clone(),
			..RbxtsOptions::default()
		}
	}
}

/// `projectOptions`: the merged option set a build runs with.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "Go's projectOptions fields")]
pub struct MergedOptions {
	/// Set only for a one-shot build: no watch, no declaration-only emit.
	pub one_shot_build: bool,
	pub include_path: String,
	pub rojo: String,
	/// `""` infers the type.
	pub type_name: String,
	pub watch: bool,
	pub use_polling: bool,
	pub verbose: bool,
	pub no_include: bool,
	pub log_truthy_changes: bool,
	pub write_only_changed: bool,
	pub write_transformed_files: bool,
	pub optimized_array_appends: bool,
	pub optimized_loops: bool,
	pub allow_comment_directives: bool,
	pub luau: bool,
	pub luau_types: bool,
	pub emit_declaration_only: bool,
	/// The argv layer of the merge.
	pub argv: Option<PartialOptions>,
	pub builders: Option<i64>,
	pub checkers: Option<i64>,
	pub single_threaded: Option<bool>,
}

impl Default for MergedOptions {
	/// `defaultProjectOptions` (`DEFAULT_PROJECT_OPTIONS`): only `optimizedLoops` and `luau` are on.
	fn default() -> Self {
		Self {
			one_shot_build: false,
			include_path: String::new(),
			rojo: String::new(),
			type_name: String::new(),
			watch: false,
			use_polling: false,
			verbose: false,
			no_include: false,
			log_truthy_changes: false,
			write_only_changed: false,
			write_transformed_files: false,
			optimized_array_appends: false,
			optimized_loops: true,
			allow_comment_directives: false,
			luau: true,
			luau_types: false,
			emit_declaration_only: false,
			argv: None,
			builders: None,
			checkers: None,
			single_threaded: None,
		}
	}
}

/// `mergeProjectOptions`: `Object.assign` over the layers; later layers win and absent fields keep
/// the earlier value.
#[must_use]
pub fn merge_project_options(base: MergedOptions, layers: &[Option<&PartialOptions>]) -> MergedOptions {
	let mut out = base;
	for layer in layers.iter().flatten() {
		if let Some(value) = &layer.include_path {
			out.include_path.clone_from(value);
		}
		if let Some(value) = &layer.rojo {
			out.rojo.clone_from(value);
		}
		if let Some(value) = &layer.type_name {
			out.type_name.clone_from(value);
		}
		let flags = [
			(&mut out.watch, layer.watch),
			(&mut out.use_polling, layer.use_polling),
			(&mut out.verbose, layer.verbose),
			(&mut out.no_include, layer.no_include),
			(&mut out.log_truthy_changes, layer.log_truthy_changes),
			(&mut out.write_only_changed, layer.write_only_changed),
			(&mut out.write_transformed_files, layer.write_transformed_files),
			(&mut out.optimized_array_appends, layer.optimized_array_appends),
			(&mut out.optimized_loops, layer.optimized_loops),
			(&mut out.allow_comment_directives, layer.allow_comment_directives),
			(&mut out.luau, layer.luau),
			(&mut out.luau_types, layer.luau_types),
		];
		for (field, value) in flags {
			if let Some(value) = value {
				*field = value;
			}
		}
	}
	out
}

/// `findTsConfigPath`: the argument when it is a file, else the nearest `tsconfig.json` from it
/// upward. The failure is upstream's exact `CLIError` text.
///
/// # Errors
///
/// `Unable to find tsconfig.json!`, or the working directory cannot be read.
pub fn find_ts_config_path(project: &str) -> Result<String, String> {
	let resolved = gopath::abs(project).map_err(|error| error.to_string())?;
	if std::fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_file()) {
		return Ok(resolved);
	}
	let mut dir = resolved;
	loop {
		let candidate = gopath::join(&[&dir, "tsconfig.json"]);
		if std::fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
			return Ok(candidate);
		}
		let parent = gopath::dir(&dir);
		if parent == dir {
			return Err("Unable to find tsconfig.json!".to_owned());
		}
		dir = parent;
	}
}

/// `readRbxtsOptionsChecked`: the tsconfig's `rbxts` layer, `None` when it declares none.
///
/// # Errors
///
/// An unparsable config or an invalid `rbxts` value.
pub fn read_rbxts_layer(ts_config_path: &str) -> Result<Option<PartialOptions>, String> {
	let options = read_rbxts_options(ts_config_path).map_err(|error| error.to_string())?;
	Ok(options.map(|rbxts| PartialOptions {
		include_path: rbxts.include_path,
		rojo: rbxts.rojo,
		type_name: rbxts.project_type,
		no_include: rbxts.no_include,
		log_truthy_changes: rbxts.log_truthy_changes,
		optimized_array_appends: rbxts.optimized_array_appends,
		optimized_loops: rbxts.optimized_loops,
		allow_comment_directives: rbxts.allow_comment_directives,
		luau: rbxts.luau,
		luau_types: rbxts.luau_types,
		..PartialOptions::default()
	}))
}

/// `projectCompileOptions`: the compile layer's options for `opts`.
#[must_use]
pub fn project_compile_options(ts_config_path: &str, opts: &MergedOptions) -> ProjectOptions {
	let mut compiled = ProjectOptions::default();
	ts_config_path.clone_into(&mut compiled.ts_config_path);
	compiled.include_path.clone_from(&opts.include_path);
	compiled.emit_include_files = !opts.no_include;
	compiled.project_type = rotor_compile::project::parse_project_type(&opts.type_name);
	compiled.rojo_config_path.clone_from(&opts.rojo);
	compiled.log_truthy_changes = opts.log_truthy_changes;
	compiled.allow_comment_directives = opts.allow_comment_directives;
	compiled.optimized_array_appends = opts.optimized_array_appends;
	compiled.no_optimized_loops = !opts.optimized_loops;
	compiled.lua_extension = !opts.luau;
	compiled.luau_types = opts.luau_types;
	compiled.write_only_changed = opts.write_only_changed;
	compiled.emit_declaration_only = opts.emit_declaration_only;
	compiled.builders = opts.builders;
	compiled.checkers = opts.checkers;
	compiled.single_threaded = opts.single_threaded;
	compiled.solution_argv = opts.argv.as_ref().map(PartialOptions::rbxts_options);
	if opts.one_shot_build {
		rotor_compile::sidecar_parallel::mark_one_shot_cli_transformer_build(&mut compiled);
	}
	compiled
}

/// `watchIncludeDir`: where the build writes the runtime include folder.
#[must_use]
pub fn include_dir(project_dir: &str, opts: &MergedOptions) -> String {
	if opts.include_path.is_empty() {
		return gopath::join(&[project_dir, "include"]);
	}
	gopath::abs(&gopath::from_slash(&opts.include_path)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
	//! `options_test.go`

	use super::{MergedOptions, PartialOptions, find_ts_config_path, merge_project_options, read_rbxts_layer};

	fn merged(edit: impl FnOnce(&mut MergedOptions)) -> MergedOptions {
		let mut options = MergedOptions::default();
		edit(&mut options);
		options
	}

	#[test]
	fn merge_project_options_layers_defaults_rbxts_then_argv() {
		let cases: Vec<(Option<PartialOptions>, Option<PartialOptions>, MergedOptions)> = vec![
			(None, None, MergedOptions::default()),
			(
				Some(PartialOptions {
					luau: Some(false),
					verbose: Some(true),
					include_path: Some("inc".to_owned()),
					..PartialOptions::default()
				}),
				None,
				merged(|o| {
					o.luau = false;
					o.verbose = true;
					"inc".clone_into(&mut o.include_path);
				}),
			),
			// Absent CLI booleans do not clobber rbxts values.
			(
				Some(PartialOptions {
					optimized_array_appends: Some(true),
					optimized_loops: Some(false),
					log_truthy_changes: Some(true),
					..PartialOptions::default()
				}),
				Some(PartialOptions::default()),
				merged(|o| {
					o.optimized_array_appends = true;
					o.optimized_loops = false;
					o.log_truthy_changes = true;
				}),
			),
			// The CLI overrides rbxts.
			(
				Some(PartialOptions {
					luau: Some(false),
					include_path: Some("a".to_owned()),
					type_name: Some("model".to_owned()),
					..PartialOptions::default()
				}),
				Some(PartialOptions {
					luau: Some(true),
					include_path: Some("b".to_owned()),
					..PartialOptions::default()
				}),
				merged(|o| {
					"b".clone_into(&mut o.include_path);
					"model".clone_into(&mut o.type_name);
				}),
			),
			(
				Some(PartialOptions {
					luau_types: Some(true),
					..PartialOptions::default()
				}),
				Some(PartialOptions {
					luau_types: Some(false),
					..PartialOptions::default()
				}),
				MergedOptions::default(),
			),
		];
		for (rbxts, argv, want) in cases {
			let got = merge_project_options(MergedOptions::default(), &[rbxts.as_ref(), argv.as_ref()]);
			assert_eq!(got, want, "rbxts {rbxts:?}, argv {argv:?}");
		}
	}

	#[test]
	fn find_ts_config_path_prefers_files_then_searches_upward() {
		let temp = tempfile::tempdir().expect("temp dir");
		let root = temp.path().canonicalize().expect("canonical temp dir");
		let config = root.join("tsconfig.json");
		let custom = root.join("tsconfig.build.json");
		let nested = root.join("src").join("deep");
		std::fs::write(&config, "{}").expect("write");
		std::fs::write(&custom, "{}").expect("write");
		std::fs::create_dir_all(&nested).expect("mkdir");
		let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
		assert_eq!(find_ts_config_path(&text(&root)), Ok(text(&config)));
		assert_eq!(find_ts_config_path(&text(&custom)), Ok(text(&custom)));
		assert_eq!(find_ts_config_path(&text(&nested)), Ok(text(&config)));
	}

	#[test]
	fn read_rbxts_layer_reads_the_key_and_resolves_paths() {
		let dir = tempfile::tempdir().expect("temp dir");
		let path = dir.path().join("a.json");
		std::fs::write(
			&path,
			r#"{
			// jsonc comment
			"compilerOptions": { "strict": true },
			"rbxts": { "luau": false, "optimizedArrayAppends": true, "luauTypes": true, "type": "model", "includePath": "runtime", },
		}"#,
		)
		.expect("write");
		let layer = read_rbxts_layer(&path.to_string_lossy())
			.expect("readable")
			.expect("an rbxts layer");
		assert_eq!(layer.luau, Some(false));
		assert_eq!(layer.optimized_array_appends, Some(true));
		assert_eq!(layer.luau_types, Some(true));
		assert_eq!(layer.type_name.as_deref(), Some("model"));
		assert_eq!(
			layer.include_path.as_deref(),
			Some(dir.path().join("runtime").to_string_lossy().as_ref())
		);
		assert_eq!(layer.watch, None);
		assert_eq!(layer.optimized_loops, None);

		let none = dir.path().join("b.json");
		std::fs::write(&none, r#"{"compilerOptions": {}}"#).expect("write");
		assert_eq!(read_rbxts_layer(&none.to_string_lossy()), Ok(None));
		assert_eq!(
			read_rbxts_layer(&dir.path().join("missing.json").to_string_lossy()),
			Ok(None)
		);
	}
}
