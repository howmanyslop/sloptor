//! `packageexports.go`: the types-entry to shipped-runtime mapping the transformer uses for
//! `node_modules` imports, from each package's `exports`, `types`/`typings` and `main`.

use rotor_rojo::canonical_file_name;
use rotor_support::gopath;
use rotor_transformer::{NodeModulesPathMapping, NodeModulesPathPattern};

use crate::gojson::{self, Value};
use crate::packageexports_parse::{export_mappings, has_package_exports};
use crate::project::resolve_against;

/// `packageManifest`: the `package.json` fields the mapping reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct PackageManifest {
	pub(crate) version: String,
	pub(crate) main: String,
	pub(crate) types: String,
	pub(crate) typings: String,
	pub(crate) exports: Option<Value>,
}

/// `packageExportMapping`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PackageExportMapping {
	pub(crate) types_path: String,
	pub(crate) runtime_path: String,
	pub(crate) subpath: String,
	pub(crate) custom: bool,
}

/// `resolvePackageExports`: the `.` export's types and runtime paths.
pub(crate) fn resolve_package_exports(pkg_dir: &str, custom_conditions: &[String]) -> Result<(String, String), String> {
	let pkg = read_package_manifest(pkg_dir)?;
	for mapping in export_mappings(pkg_dir, &pkg, custom_conditions) {
		if mapping.subpath == "." && !mapping.custom {
			return Ok((mapping.types_path, mapping.runtime_path));
		}
	}
	if has_package_exports(pkg.exports.as_ref()) {
		return Ok((String::new(), String::new()));
	}
	Ok(legacy_package_paths(pkg_dir, &pkg))
}

/// A string field of a Go struct decode: absent or `null` is empty; any other type fails.
fn string_field(value: &Value, key: &str) -> Result<String, String> {
	match value.field(key) {
		None | Some(Value::Null) => Ok(String::new()),
		Some(Value::String(text)) => Ok(text.clone()),
		Some(_) => Err(format!(
			"json: cannot unmarshal into Go struct field packageManifest.{key} of type string"
		)),
	}
}

/// `readPackageManifest`
pub(crate) fn read_package_manifest(pkg_dir: &str) -> Result<PackageManifest, String> {
	let data = std::fs::read(gopath::join(&[pkg_dir, "package.json"])).map_err(|error| error.to_string())?;
	let value = gojson::parse(&data)?;
	if !matches!(value, Value::Object(_) | Value::Null) {
		return Err("json: cannot unmarshal into Go value of type compile.packageManifest".to_owned());
	}
	Ok(PackageManifest {
		version: string_field(&value, "version")?,
		main: string_field(&value, "main")?,
		types: string_field(&value, "types")?,
		typings: string_field(&value, "typings")?,
		exports: value.field("exports").cloned(),
	})
}

/// `packageMappings`: the export mappings, or the legacy fields' one, plus the legacy types
/// entry as a custom alias when the exports do not already cover it.
pub(crate) fn package_mappings(
	pkg_dir: &str,
	pkg: &PackageManifest,
	custom_conditions: &[String],
) -> Vec<PackageExportMapping> {
	let mut mappings = export_mappings(pkg_dir, pkg, custom_conditions);
	if mappings.is_empty() {
		let (types_path, runtime_path) = legacy_package_paths(pkg_dir, pkg);
		if runtime_path.is_empty() {
			return Vec::new();
		}
		return vec![PackageExportMapping {
			types_path,
			runtime_path,
			subpath: ".".to_owned(),
			custom: false,
		}];
	}

	let legacy_types = if pkg.types.is_empty() { &pkg.typings } else { &pkg.types };
	if legacy_types.is_empty() || pkg.main.is_empty() {
		return mappings;
	}
	let legacy_types = resolve_against(pkg_dir, legacy_types);
	if mappings.iter().any(|mapping| mapping.types_path == legacy_types) {
		return mappings;
	}
	mappings.push(PackageExportMapping {
		types_path: legacy_types,
		runtime_path: resolve_against(pkg_dir, &pkg.main),
		subpath: ".".to_owned(),
		custom: true,
	});
	mappings
}

/// `legacyPackagePaths`: `types`/`typings` (default `index.d.ts`) and `main`.
pub(crate) fn legacy_package_paths(pkg_dir: &str, pkg: &PackageManifest) -> (String, String) {
	let mut types_path = if pkg.types.is_empty() {
		pkg.typings.as_str()
	} else {
		pkg.types.as_str()
	};
	if types_path.is_empty() {
		types_path = "index.d.ts";
	}
	if pkg.main.is_empty() {
		return (resolve_against(pkg_dir, types_path), String::new());
	}
	(
		resolve_against(pkg_dir, types_path),
		resolve_against(pkg_dir, &pkg.main),
	)
}

/// `createNodeModulesPathMapping`
pub(crate) fn create_node_modules_path_mapping(
	type_roots: &[String],
	use_case_sensitive_file_names: bool,
	custom_conditions: &[String],
) -> NodeModulesPathMapping {
	build_node_modules_path_mappings(
		type_roots,
		use_case_sensitive_file_names,
		custom_conditions,
		&remap_to_rojo_tree,
	)
	.runtime
}

/// `nodeModulesPathMappings`
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeModulesPathMappings {
	pub(crate) runtime: NodeModulesPathMapping,
	pub(crate) portable: NodeModulesPathMapping,
	pub(crate) manifests: Vec<String>,
}

/// A runtime-path remap: `(pkg_dir, runtime_path) -> runtime_path`.
pub(crate) type Remap<'a> = &'a dyn Fn(&str, &str) -> String;

/// `buildNodeModulesPathMappings`: every package of every type root.
pub(crate) fn build_node_modules_path_mappings(
	type_roots: &[String],
	use_case_sensitive_file_names: bool,
	custom_conditions: &[String],
	remap: Remap<'_>,
) -> NodeModulesPathMappings {
	let mut mapping = NodeModulesPathMapping::default();
	let mut portable = NodeModulesPathMapping::default();
	let mut manifests = Vec::new();
	for type_root in type_roots {
		let scope_path = gopath::from_slash(type_root).into_owned();
		let Ok(entries) = read_dir_sorted(&scope_path) else {
			continue;
		};
		for name in entries {
			let pkg_dir = gopath::join(&[&scope_path, &name]);
			manifests.push(gopath::join(&[&pkg_dir, "package.json"]));
			let Ok(pkg) = read_package_manifest(&pkg_dir) else {
				continue;
			};
			let virtual_mappings = package_mappings(&pkg_dir, &pkg, custom_conditions);
			add_package_mappings(
				&mut mapping,
				&pkg_dir,
				&virtual_mappings,
				use_case_sensitive_file_names,
				remap,
			);
			// Fingerprints keep the authored spelling and logical package paths.
			add_package_mappings(&mut portable, &pkg_dir, &virtual_mappings, true, remap);

			let real_pkg_dir = package_real_path(&pkg_dir);
			if canonical_file_name(&real_pkg_dir, use_case_sensitive_file_names)
				== canonical_file_name(&pkg_dir, use_case_sensitive_file_names)
			{
				continue;
			}
			let real_mappings = package_mappings(&real_pkg_dir, &pkg, custom_conditions);
			add_real_path_aliases(
				&mut mapping,
				&pkg_dir,
				&real_mappings,
				&virtual_mappings,
				use_case_sensitive_file_names,
				remap,
			);
		}
	}
	NodeModulesPathMappings {
		runtime: mapping,
		portable,
		manifests,
	}
}

/// `os.ReadDir`: the entry names, sorted.
pub(crate) fn read_dir_sorted(dir: &str) -> std::io::Result<Vec<String>> {
	let mut names: Vec<String> = std::fs::read_dir(dir)?
		.filter_map(Result::ok)
		.map(|entry| entry.file_name().to_string_lossy().into_owned())
		.collect();
	names.sort();
	Ok(names)
}

/// `addPackageMappings`
fn add_package_mappings(
	mapping: &mut NodeModulesPathMapping,
	pkg_dir: &str,
	mappings: &[PackageExportMapping],
	use_case_sensitive_file_names: bool,
	remap: Remap<'_>,
) {
	for entry in mappings {
		if entry.types_path.contains('*') {
			add_package_pattern(
				mapping,
				pkg_dir,
				&entry.types_path,
				&entry.runtime_path,
				use_case_sensitive_file_names,
				remap,
			);
			continue;
		}
		let key = canonical_file_name(&entry.types_path, use_case_sensitive_file_names);
		if entry.custom && mapping.exact.contains_key(&key) {
			continue;
		}
		mapping.exact.insert(key, remap(pkg_dir, &entry.runtime_path));
	}
}

/// `addRealPathAliases`
fn add_real_path_aliases(
	mapping: &mut NodeModulesPathMapping,
	virtual_pkg_dir: &str,
	real_mappings: &[PackageExportMapping],
	virtual_mappings: &[PackageExportMapping],
	use_case_sensitive_file_names: bool,
	remap: Remap<'_>,
) {
	for (real_mapping, virtual_mapping) in real_mappings.iter().zip(virtual_mappings) {
		if real_mapping.types_path.contains('*') {
			add_package_pattern(
				mapping,
				virtual_pkg_dir,
				&real_mapping.types_path,
				&virtual_mapping.runtime_path,
				use_case_sensitive_file_names,
				remap,
			);
			continue;
		}
		let key = canonical_file_name(&real_mapping.types_path, use_case_sensitive_file_names);
		mapping
			.exact
			.entry(key)
			.or_insert_with(|| remap(virtual_pkg_dir, &virtual_mapping.runtime_path));
	}
}

/// `addPackagePattern`: a wildcard export subpath. The types target must hold exactly one `*`;
/// the Rojo tree remap is skipped when the `*` sits in the first runtime segment.
fn add_package_pattern(
	mapping: &mut NodeModulesPathMapping,
	pkg_dir: &str,
	types_path: &str,
	runtime_path: &str,
	use_case_sensitive_file_names: bool,
	remap: Remap<'_>,
) {
	if types_path.matches('*').count() != 1 {
		return;
	}
	let canonical = canonical_file_name(types_path, use_case_sensitive_file_names);
	let (prefix, suffix) = canonical.split_once('*').unwrap_or((canonical.as_str(), ""));
	// The Rojo tree remap is skipped when the `*` sits in the first runtime segment.
	let remaps = gopath::rel(pkg_dir, runtime_path).is_ok_and(|rel| {
		!rel.split(gopath::NATIVE.separator())
			.next()
			.unwrap_or_default()
			.contains('*')
	});
	let runtime = if remaps {
		remap(pkg_dir, runtime_path)
	} else {
		runtime_path.to_owned()
	};
	mapping.patterns.push(NodeModulesPathPattern {
		types_prefix: prefix.to_owned(),
		types_suffix: suffix.to_owned(),
		runtime,
	});
}

/// `remapToRojoTree`: a runtime path outside the package's Rojo tree `$path` is moved into it.
pub(crate) fn remap_to_rojo_tree(pkg_dir: &str, runtime_path: &str) -> String {
	let (Some(project_path), _) = rotor_rojo::find_rojo_config_file_path(pkg_dir) else {
		return runtime_path.to_owned();
	};
	let Ok((_, tree)) = rotor_rojo::parse_project_file(&project_path) else {
		return runtime_path.to_owned();
	};
	if tree.path.is_none() {
		return runtime_path.to_owned();
	}
	let Ok(data) = std::fs::read(&project_path) else {
		return runtime_path.to_owned();
	};
	let Ok(project) = gojson::parse(&data) else {
		return runtime_path.to_owned();
	};
	let Some(Value::String(rojo_tree_path)) = project.field("tree").and_then(|tree| tree.field("$path")) else {
		return runtime_path.to_owned();
	};
	remap_runtime_path(pkg_dir, runtime_path, rojo_tree_path)
}

/// `remapRuntimePath`: replaces the first segment of a runtime path outside the Rojo tree
/// directory with the tree's `$path`.
pub(crate) fn remap_runtime_path(pkg_dir: &str, runtime_path: &str, rojo_tree_path: &str) -> String {
	let rojo_dir = resolve_against(pkg_dir, rojo_tree_path);
	let separator = gopath::NATIVE.separator();
	if let Ok(rel) = gopath::rel(&rojo_dir, &gopath::clean(runtime_path))
		&& rel != ".."
		&& !rel.starts_with(&format!("..{separator}"))
	{
		return runtime_path.to_owned();
	}
	let Ok(rel_runtime) = gopath::rel(pkg_dir, runtime_path) else {
		return runtime_path.to_owned();
	};
	let mut segments: Vec<String> = rel_runtime.split(separator).map(str::to_owned).collect();
	segments[0] = gopath::from_slash(rojo_tree_path).into_owned();
	let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
	gopath::join(&[pkg_dir, &gopath::join(&segments)])
}

/// `packageRealPath`
pub(crate) fn package_real_path(pkg_dir: &str) -> String {
	crate::sidecar::realpath_or(pkg_dir)
}
