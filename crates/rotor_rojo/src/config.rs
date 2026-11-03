//! Rojo project-file parsing: an insertion-ordered JSON reader plus a hand
//! validator mirroring `rojo-schema.json` (upstream validates with ajv;
//! `reference/rojo-resolver/src/RojoResolver.ts` L148-152). Key order matters
//! because `parseTree` walks `Object.keys(tree)`, which drives partition LIFO
//! ordering, so an unordered map cannot be used.

use crate::json::JsonValue;

/// A parsed Rojo project tree node (`RojoResolver.ts` `RojoTree`, L21-37).
/// Public so `from_tree` callers can build trees programmatically (upstream
/// `fromTree`'s only call site is the playground `VirtualProject`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
	/// `$className` (empty when absent).
	pub class_name: String,
	/// `$path`; for the object form, `$path.optional`. `None` when absent.
	pub path: Option<String>,
	/// The non-`$`-prefixed members in JavaScript property order
	/// (array-index-like keys first ascending, then insertion order).
	pub children: Vec<TreeEntry>,
}

/// One named child of a [`Tree`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
	/// The child's instance name.
	pub name: String,
	/// The child's subtree.
	pub tree: Tree,
}

/// Mirrors ajv validation of `rojo-schema.json` over the whole config
/// document.
///
/// # Errors
///
/// Returns a description shaped like ajv's `errorsText` (`data/tree must be
/// object`). The exact wording is not byte-identical to ajv; it is warning
/// text only and never reaches compiled output.
pub fn validate_config(v: &JsonValue) -> Result<(), String> {
	if !matches!(v, JsonValue::Object(_)) {
		return Err("data must be object".to_owned());
	}
	for required in ["name", "tree"] {
		if v.member(required).is_none() {
			return Err(format!("data must have required property '{required}'"));
		}
	}
	let JsonValue::Object(members) = v else {
		return Ok(());
	};
	for (key, value) in members {
		match key.as_str() {
			"name" => {
				if !matches!(value, JsonValue::String(_)) {
					return Err("data/name must be string".to_owned());
				}
			}
			"servePort" => {
				if !matches!(value, JsonValue::Number { value, .. } if is_integer(*value)) {
					return Err("data/servePort must be integer".to_owned());
				}
			}
			"tree" => validate_tree(value, "data/tree")?,
			_ => {}
		}
	}
	Ok(())
}

/// `Math.trunc(n) === n`, which the schema's `integer` type means.
#[expect(
	clippy::float_cmp,
	reason = "an exact comparison is the definition of an integral double"
)]
fn is_integer(n: f64) -> bool {
	n.trunc() == n
}

fn validate_tree(v: &JsonValue, path: &str) -> Result<(), String> {
	let JsonValue::Object(members) = v else {
		return Err(format!("{path} must be object"));
	};
	for (key, value) in members {
		match key.as_str() {
			"$className" => {
				if !matches!(value, JsonValue::String(_)) {
					return Err(format!("{path}/$className must be string"));
				}
			}
			"$ignoreUnknownInstances" => {
				if !matches!(value, JsonValue::Bool(_)) {
					return Err(format!("{path}/$ignoreUnknownInstances must be boolean"));
				}
			}
			"$path" => {
				if !is_valid_path_value(value) {
					return Err(format!("{path}/$path must match a schema in anyOf"));
				}
			}
			"$properties" => {
				if !matches!(value, JsonValue::Object(_)) {
					return Err(format!("{path}/$properties must be object"));
				}
			}
			// patternProperties ^[^\$].*$: non-empty keys not starting with "$"
			// must be trees; other keys are unvalidated.
			key if !key.is_empty() && !key.starts_with('$') => validate_tree(value, &format!("{path}/{key}"))?,
			_ => {}
		}
	}
	Ok(())
}

/// Checks the `$path` anyOf: a string, or an object whose `optional` member
/// (if present) is a string.
fn is_valid_path_value(v: &JsonValue) -> bool {
	match v {
		JsonValue::String(_) => true,
		JsonValue::Object(_) => v
			.member("optional")
			.is_none_or(|optional| matches!(optional, JsonValue::String(_))),
		_ => false,
	}
}

/// Extracts `(name, tree)` from a validated config document.
pub fn config_from_json(v: &JsonValue) -> (String, Tree) {
	let name = v
		.member("name")
		.and_then(JsonValue::as_str)
		.unwrap_or_default()
		.to_owned();
	let tree = v.member("tree").map(tree_from_json).unwrap_or_default();
	(name, tree)
}

fn tree_from_json(v: &JsonValue) -> Tree {
	let mut tree = Tree::default();
	let JsonValue::Object(members) = v else {
		// Only reachable for ""-keyed children (which the schema does not
		// validate). Upstream would iterate Object.keys of the primitive;
		// this treats it as a childless tree instead.
		return tree;
	};
	for (key, value) in members {
		if key == "$className" {
			value.as_str().unwrap_or_default().clone_into(&mut tree.class_name);
		} else if key == "$path" {
			if let JsonValue::String(s) = value {
				tree.path = Some(s.clone());
			} else if let Some(optional) = value.member("optional") {
				tree.path = Some(optional.as_str().unwrap_or_default().to_owned());
			}
			// The object form without "optional" validates upstream but would
			// crash parseTree (path.resolve(basePath, undefined) throws); this
			// treats it as absent instead.
		} else if !key.starts_with('$') {
			tree.children.push(TreeEntry {
				name: key.clone(),
				tree: tree_from_json(value),
			});
		}
	}
	tree
}
