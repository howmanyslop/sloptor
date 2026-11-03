//! `packageexports_parse.go`: resolving a package's `exports` field to its types and runtime
//! targets, conditions in source order.

use std::collections::HashSet;

use rotor_support::gopath;

use crate::gojson::Value;
use crate::packageexports::{PackageExportMapping, PackageManifest};
use crate::project::resolve_against;

/// `resolvedExportTargets`
#[derive(Clone, Debug, Default)]
struct ResolvedExportTargets {
	types: Option<String>,
	runtime: Option<String>,
	custom_targets: Vec<String>,
	/// Set once a custom condition branch matched: TypeScript stops at that branch, so runtime
	/// conditions after it must not fill a missing runtime target.
	runtime_decided: bool,
}

/// `hasPackageExports`: an `exports` field that is present and not `null`.
pub(crate) fn has_package_exports(exports: Option<&Value>) -> bool {
	exports.is_some_and(|exports| *exports != Value::Null)
}

/// `exportMappings`
pub(crate) fn export_mappings(
	pkg_dir: &str,
	pkg: &PackageManifest,
	custom_conditions: &[String],
) -> Vec<PackageExportMapping> {
	let Some(exports) = pkg
		.exports
		.as_ref()
		.filter(|_| has_package_exports(pkg.exports.as_ref()))
	else {
		return Vec::new();
	};
	let customs: HashSet<&str> = custom_conditions.iter().map(String::as_str).collect();
	match exports {
		Value::String(_) | Value::Array(_) => resolve_export_subpath(pkg_dir, pkg, ".", exports, &customs),
		Value::Object(entries) => {
			let is_subpath_map = !entries.is_empty() && entries.iter().all(|(key, _)| key.starts_with('.'));
			if !is_subpath_map {
				return resolve_export_subpath(pkg_dir, pkg, ".", exports, &customs);
			}
			entries
				.iter()
				.flat_map(|(key, value)| resolve_export_subpath(pkg_dir, pkg, key, value, &customs))
				.collect()
		}
		_ => Vec::new(),
	}
}

/// `resolveExportSubpath`
fn resolve_export_subpath(
	pkg_dir: &str,
	pkg: &PackageManifest,
	subpath: &str,
	value: &Value,
	customs: &HashSet<&str>,
) -> Vec<PackageExportMapping> {
	if *value == Value::Null {
		return Vec::new();
	}
	let targets = resolve_export_targets(value, customs);
	let Some(runtime) = targets.runtime else {
		return Vec::new();
	};
	let mut types = targets.types;
	if types.is_none() {
		if subpath == "." {
			let legacy = if pkg.types.is_empty() { &pkg.typings } else { &pkg.types };
			types = (!legacy.is_empty()).then(|| legacy.clone());
		}
		if types.is_none() {
			types = Some(adjacent_dts(&runtime));
		}
	}

	let runtime_path = resolve_against(pkg_dir, &runtime);
	let first = PackageExportMapping {
		types_path: resolve_against(pkg_dir, &types.unwrap_or_default()),
		runtime_path: runtime_path.clone(),
		subpath: subpath.to_owned(),
		custom: false,
	};
	let mut mappings = vec![first];
	for custom_target in &targets.custom_targets {
		let custom_path = resolve_against(pkg_dir, custom_target);
		if custom_path != mappings[0].types_path {
			mappings.push(PackageExportMapping {
				types_path: custom_path,
				runtime_path: runtime_path.clone(),
				subpath: subpath.to_owned(),
				custom: true,
			});
		}
	}
	mappings
}

/// `resolveExportTargets`
fn resolve_export_targets(value: &Value, customs: &HashSet<&str>) -> ResolvedExportTargets {
	match value {
		Value::String(text) => ResolvedExportTargets {
			runtime: Some(text.clone()),
			..ResolvedExportTargets::default()
		},
		Value::Object(entries) => resolve_export_condition_object(entries, customs),
		Value::Array(elements) => {
			let mut result = ResolvedExportTargets::default();
			for entry in elements {
				match entry {
					Value::String(text) => {
						result.runtime = Some(text.clone());
						return result;
					}
					Value::Object(entries) => {
						merge_export_targets(&mut result, resolve_export_condition_object(entries, customs));
						if result.runtime.is_some() {
							return result;
						}
					}
					_ => {}
				}
			}
			result
		}
		_ => ResolvedExportTargets::default(),
	}
}

/// `resolveExportConditionObject`
fn resolve_export_condition_object(entries: &[(String, Value)], customs: &HashSet<&str>) -> ResolvedExportTargets {
	let mut result = ResolvedExportTargets::default();
	for (key, value) in entries {
		let custom = customs.contains(key.as_str());
		match value {
			Value::String(text) if key == "types" && result.types.is_none() => {
				result.types = Some(text.clone());
			}
			Value::String(text) if custom => result.custom_targets.push(text.clone()),
			Value::Object(inner) if custom => {
				let inner = resolve_export_condition_object(inner, customs);
				if inner.types.is_none() && inner.runtime.is_none() && inner.custom_targets.is_empty() {
					continue;
				}
				if result.types.is_none() && inner.types.is_some() {
					result.types = inner.types;
				}
				if result.runtime.is_none() && !result.runtime_decided && inner.runtime.is_some() {
					result.runtime = inner.runtime;
				}
				result.custom_targets.extend(inner.custom_targets);
				result.runtime_decided = true;
			}
			_ if is_runtime_condition(key) && !result.runtime_decided => match value {
				Value::String(text) if result.runtime.is_none() => result.runtime = Some(text.clone()),
				Value::Object(inner) => {
					merge_export_targets(&mut result, resolve_export_condition_object(inner, customs));
				}
				_ => {}
			},
			_ => {}
		}
	}
	result
}

/// `mergeExportTargets`
fn merge_export_targets(result: &mut ResolvedExportTargets, inner: ResolvedExportTargets) {
	if result.types.is_none() && inner.types.is_some() {
		result.types = inner.types;
	}
	if result.runtime.is_none() && !result.runtime_decided && inner.runtime.is_some() {
		result.runtime = inner.runtime;
	}
	result.runtime_decided = result.runtime_decided || inner.runtime_decided;
	result.custom_targets.extend(inner.custom_targets);
}

/// `isRuntimeCondition`
fn is_runtime_condition(condition: &str) -> bool {
	matches!(condition, "node" | "require" | "import" | "default")
}

/// `adjacentDTS`: the path with its extension replaced by `.d.ts`.
pub(crate) fn adjacent_dts(file_path: &str) -> String {
	let extension = extension(file_path);
	format!("{}.d.ts", &file_path[..file_path.len() - extension.len()])
}

/// `filepath.Ext`
pub(crate) fn extension(path: &str) -> &str {
	let separator = gopath::NATIVE.separator();
	for (index, character) in path.char_indices().rev() {
		if character == '/' || character == separator {
			break;
		}
		if character == '.' {
			return &path[index..];
		}
	}
	""
}
