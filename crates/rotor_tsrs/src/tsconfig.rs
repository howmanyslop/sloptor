//! What rotor does to a tsconfig before tsrs sees it, ported from `internal/compile`.
//!
//! rbxtsc requires options that TypeScript 7 removed, so every config in the extends chain is
//! rewritten on read ([`sanitize_tsconfig`]), and `@rbxts/compiler-types` gets the iteration
//! interfaces TypeScript 7 expects ([`rewrite_iterable_arity`]). The raw values the rewrite drops are
//! read separately ([`read_raw_enforced_options`]) so [`validate_compiler_options`] can report
//! rbxtsc's exact errors.

use std::path::Path as StdPath;
use std::sync::LazyLock;

use regex::Regex;
use rotor_support::gopath;
use serde_json::{Map, Value};
use tsrs_core::{CompilerOptions, ModuleDetectionKind, ModuleKind, ModuleResolutionKind, Tristate};

/// The four arity-1 iteration interfaces `@rbxts/compiler-types` declares (`sanitize.go`
/// `iterableArityPattern`).
static ITERABLE_ARITY: LazyLock<Option<Regex>> = LazyLock::new(|| {
	Regex::new(r"\binterface (Iterable|IterableIterator|AsyncIterable|AsyncIterableIterator)<T>").ok()
});

/// Rewrites `@rbxts/compiler-types`' arity-1 iteration interfaces to `<T, TReturn = any, TNext =
/// any>`, the arity TypeScript 7 resolves the iteration globals at (`RewriteIterableArity`).
#[must_use]
pub fn rewrite_iterable_arity(source: &str) -> String {
	ITERABLE_ARITY.as_ref().map_or_else(
		|| source.to_owned(),
		|pattern| {
			pattern
				.replace_all(source, "interface $1<T, TReturn = any, TNext = any>")
				.into_owned()
		},
	)
}

/// Whether `path` is a declaration file of `@rbxts/compiler-types` (`isCompilerTypesDTSPath`).
#[must_use]
pub fn is_compiler_types_dts_path(path: &str) -> bool {
	path.contains("/node_modules/@rbxts/compiler-types/") && path.ends_with(".d.ts")
}

/// Whether `path`'s base name is exactly `tsconfig.json` (`isTSConfigPath`).
#[must_use]
pub fn is_tsconfig_path(path: &str) -> bool {
	path == "tsconfig.json" || path.ends_with("/tsconfig.json")
}

/// The `\uXXXX` escape at the start of `bytes`, if there is one.
fn unicode_escape(bytes: &[u8]) -> Option<u16> {
	let digits = bytes.strip_prefix(b"\\u")?.get(..4)?;
	u16::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
}

/// Parses JSON the way Go's `encoding/json` does where `serde_json` is stricter: an escaped UTF-16
/// surrogate without its pair decodes to U+FFFD instead of failing.
fn parse_json(text: &str) -> serde_json::Result<Value> {
	let bytes = text.as_bytes();
	let mut out = String::new();
	let (mut copied, mut index) = (0, 0);
	while index < bytes.len() {
		if bytes[index] != b'\\' {
			index += 1;
			continue;
		}
		let Some(unit) = unicode_escape(&bytes[index..]) else {
			index += 2;
			continue;
		};
		let paired = (0xD800..0xDC00).contains(&unit)
			&& unicode_escape(&bytes[index + 6..]).is_some_and(|low| (0xDC00..0xE000).contains(&low));
		if paired {
			index += 12;
		} else if (0xD800..0xE000).contains(&unit) {
			out.push_str(&text[copied..index]);
			out.push_str("\\ufffd");
			index += 6;
			copied = index;
		} else {
			index += 6;
		}
	}
	if copied == 0 {
		return serde_json::from_str(text);
	}
	out.push_str(&text[copied..]);
	serde_json::from_str(&out)
}

/// Removes `//` and `/* */` comments and trailing commas outside strings, so a tsconfig parses as
/// JSON (`stripJSONC`).
#[must_use]
pub fn strip_jsonc(source: &str) -> String {
	let bytes = source.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut in_string = false;
	let mut index = 0;
	while index < bytes.len() {
		let byte = bytes[index];
		if in_string {
			out.push(byte);
			if byte == b'\\' && index + 1 < bytes.len() {
				index += 1;
				out.push(bytes[index]);
			} else if byte == b'"' {
				in_string = false;
			}
			index += 1;
			continue;
		}
		match byte {
			b'"' => {
				in_string = true;
				out.push(byte);
			}
			b'/' if bytes.get(index + 1) == Some(&b'/') => {
				while index < bytes.len() && bytes[index] != b'\n' {
					index += 1;
				}
				if index < bytes.len() {
					out.push(b'\n');
				}
			}
			b'/' if bytes.get(index + 1) == Some(&b'*') => {
				index += 2;
				while index + 1 < bytes.len() && (bytes[index] != b'*' || bytes[index + 1] != b'/') {
					index += 1;
				}
				// Land on the closing '/'; the increment below steps past it.
				index += 1;
			}
			b',' => {
				let next = skip_jsonc_trivia(bytes, index + 1);
				if !matches!(bytes.get(next), Some(b'}' | b']')) {
					out.push(byte);
				}
			}
			_ => out.push(byte),
		}
		index += 1;
	}
	String::from_utf8(out).unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

fn skip_jsonc_trivia(bytes: &[u8], mut index: usize) -> usize {
	while index < bytes.len() {
		match bytes[index] {
			b' ' | b'\t' | b'\r' | b'\n' => index += 1,
			b'/' if bytes.get(index + 1) == Some(&b'/') => {
				index += 2;
				while index < bytes.len() && bytes[index] != b'\n' {
					index += 1;
				}
			}
			b'/' if bytes.get(index + 1) == Some(&b'*') => {
				index += 2;
				while index + 1 < bytes.len() && (bytes[index] != b'*' || bytes[index + 1] != b'/') {
					index += 1;
				}
				if index + 1 < bytes.len() {
					index += 2;
				}
			}
			_ => return index,
		}
	}
	index
}

/// Rewrites a tsconfig rbxtsc accepts into one TypeScript 7 accepts (`SanitizeTSConfig`).
///
/// `downlevelIteration` and `importsNotUsedAsValues` are dropped, `baseUrl` becomes the equivalent
/// `paths` wildcard, and `moduleResolution` `node`/`node10` becomes `bundler`. Malformed input comes
/// back untouched so tsrs reports the parse error itself.
#[must_use]
pub fn sanitize_tsconfig(source: &str) -> String {
	let clean = strip_jsonc(source);
	let Ok(Value::Object(mut root)) = parse_json(&clean) else {
		return source.to_owned();
	};
	let Some(Value::Object(options)) = root.get_mut("compilerOptions") else {
		return clean;
	};
	options.remove("downlevelIteration");
	options.remove("importsNotUsedAsValues");
	if let Some(Value::String(base_url)) = options.get("baseUrl").cloned() {
		inject_base_url_paths(options, &base_url);
	}
	options.remove("baseUrl");
	if let Some(Value::String(resolution)) = options.get("moduleResolution")
		&& is_node10_module_resolution_text(resolution)
	{
		options.insert("moduleResolution".to_owned(), Value::String("bundler".to_owned()));
	}
	serde_json::to_string_pretty(&Value::Object(root)).unwrap_or(clean)
}

/// Whether a raw `moduleResolution` value parses to TypeScript 5's `Node10`.
const fn is_node10_module_resolution_text(value: &str) -> bool {
	value.eq_ignore_ascii_case("node") || value.eq_ignore_ascii_case("node10")
}

/// Whether a tsconfig path text is absolute on any platform (`isAbsolutePathText`).
fn is_absolute_path_text(text: &str) -> bool {
	if text.starts_with('/') || text.starts_with('\\') {
		return true;
	}
	let bytes = text.as_bytes();
	bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

/// Replaces a removed `baseUrl` with the `paths` wildcard TypeScript 7 suggests, keeping TypeScript
/// 5's paths-relative-to-baseUrl and baseUrl-fallback semantics (`injectBaseURLPaths`).
fn inject_base_url_paths(options: &mut Map<String, Value>, base_url: &str) {
	let replaced = base_url.replace('\\', "/");
	let base = replaced.strip_prefix("./").unwrap_or(&replaced);
	let base = base.strip_suffix('/').unwrap_or(base);
	let prefix = if is_absolute_path_text(base) {
		format!("{base}/")
	} else if !base.is_empty() && base != "." {
		format!("./{base}/")
	} else {
		"./".to_owned()
	};
	let wild = format!("{prefix}*");

	let Some(paths) = options.get_mut("paths") else {
		let mut paths = Map::new();
		paths.insert("*".to_owned(), Value::Array(vec![Value::String(wild)]));
		options.insert("paths".to_owned(), Value::Object(paths));
		return;
	};
	let Value::Object(paths) = paths else {
		return;
	};
	let rebase = |text: &str| -> String {
		if is_absolute_path_text(text) {
			return text.to_owned();
		}
		let joined = format!("{prefix}{}", text.strip_prefix("./").unwrap_or(text));
		let cleaned = gopath::Flavor::Unix.clean(&joined);
		if !is_absolute_path_text(&cleaned) && !cleaned.starts_with("./") && !cleaned.starts_with("../") {
			return format!("./{cleaned}");
		}
		cleaned
	};
	for (pattern, substitutions) in paths.iter_mut() {
		let Value::Array(substitutions) = substitutions else {
			continue;
		};
		let mut rebased: Vec<Value> = substitutions
			.iter()
			.map(|substitution| match substitution {
				Value::String(text) => Value::String(rebase(text)),
				other => other.clone(),
			})
			.collect();
		if pattern != "*" {
			rebased.push(Value::String(rebase(pattern)));
		}
		*substitutions = rebased;
	}
	match paths.get_mut("*") {
		Some(Value::Array(star)) => star.push(Value::String(wild)),
		Some(_) => {}
		None => {
			paths.insert("*".to_owned(), Value::Array(vec![Value::String(wild)]));
		}
	}
}

/// The raw extends-chain values of the options the sanitizer rewrites or drops, which
/// [`validate_compiler_options`] checks against rbxtsc's rules (`rawEnforcedOptions`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawEnforcedOptions {
	/// The raw `moduleResolution` text, when it is a string.
	pub module_resolution: Option<String>,
	/// Whether the chain sets `moduleResolution` at all.
	pub has_module_resolution: bool,
	/// The raw `types` entries that are strings.
	pub types: Vec<String>,
	/// Whether the chain sets `types` at all.
	pub has_types: bool,
	/// The raw `importsNotUsedAsValues` text, when it is a string.
	pub imports_not_used: Option<String>,
	/// Whether the chain sets `importsNotUsedAsValues` at all.
	pub has_imports_not_used: bool,
}

impl RawEnforcedOptions {
	fn merge(mut self, overlay: Self) -> Self {
		if overlay.has_module_resolution {
			self.module_resolution = overlay.module_resolution;
			self.has_module_resolution = true;
		}
		if overlay.has_types {
			self.types = overlay.types;
			self.has_types = true;
		}
		if overlay.has_imports_not_used {
			self.imports_not_used = overlay.imports_not_used;
			self.has_imports_not_used = true;
		}
		self
	}
}

fn read_config_object(path: &str) -> Option<Map<String, Value>> {
	let text = std::fs::read_to_string(path).ok()?;
	match parse_json(&strip_jsonc(&text)).ok()? {
		Value::Object(root) => Some(root),
		_ => None,
	}
}

/// Reads the raw enforced options merged down the extends chain of `config_path`
/// (`readRawEnforcedOptions`). Unreadable configs give the defaults so tsrs reports the error.
#[must_use]
pub fn read_raw_enforced_options(config_path: &str) -> RawEnforcedOptions {
	let mut visited = Vec::new();
	read_raw_enforced_options_from_chain(config_path, &mut visited)
}

fn read_raw_enforced_options_from_chain(config_path: &str, visited: &mut Vec<String>) -> RawEnforcedOptions {
	let Ok(normalized) = gopath::abs(config_path) else {
		return RawEnforcedOptions::default();
	};
	if visited.contains(&normalized) {
		return RawEnforcedOptions::default();
	}
	visited.push(normalized.clone());
	let Some(root) = read_config_object(config_path) else {
		return RawEnforcedOptions::default();
	};
	let base = match root.get("extends") {
		Some(Value::String(extends)) => resolve_extends_path(&gopath::dir(&normalized), extends)
			.ok()
			.map(|parent| read_raw_enforced_options_from_chain(&parent, visited))
			.unwrap_or_default(),
		_ => RawEnforcedOptions::default(),
	};
	let Some(Value::Object(options)) = root.get("compilerOptions") else {
		return base;
	};
	let mut current = RawEnforcedOptions::default();
	if let Some(value) = options.get("moduleResolution") {
		current.has_module_resolution = true;
		current.module_resolution = value.as_str().map(str::to_owned);
	}
	if let Some(value) = options.get("types") {
		current.has_types = true;
		current.types = value
			.as_array()
			.map(|entries| {
				entries
					.iter()
					.filter_map(|entry| entry.as_str().map(str::to_owned))
					.collect()
			})
			.unwrap_or_default();
	}
	if let Some(value) = options.get("importsNotUsedAsValues") {
		current.has_imports_not_used = true;
		current.imports_not_used = value.as_str().map(str::to_owned);
	}
	base.merge(current)
}

/// The config files of the extends chain of `config_path`, starting with it, as absolute native
/// paths (the chain half of `ReadRbxtsOptionsWithChain`).
///
/// A cycle or an `extends` that fails to resolve ends the chain; Go's callers of the chain drop
/// the error and keep what it holds.
#[must_use]
pub fn extends_chain(config_path: &str) -> Vec<String> {
	let mut chain = Vec::new();
	let mut next = gopath::abs(config_path).ok();
	while let Some(path) = next.take() {
		let path = gopath::clean(&path);
		if chain.contains(&path) {
			break;
		}
		chain.push(path.clone());
		let Some(root) = read_config_object(&path) else {
			break;
		};
		if let Some(Value::String(extends)) = root.get("extends") {
			next = resolve_extends_path(&gopath::dir(&path), extends).ok();
		}
	}
	chain
}

/// Resolves an `extends` value the way `resolveExtendedConfig` does: a package specifier through
/// tsconfig module resolution, a path relative to `directory` otherwise (adding `.json` if needed).
///
/// # Errors
///
/// Fails, with Go's message, on an empty value, an unresolvable package, a missing file, or a
/// `stat` that fails for another reason than the file not existing.
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go's resolveExtendedConfig appends .json unless the path ends with it exactly"
)]
pub fn resolve_extends_path(directory: &str, extends: &str) -> Result<String, String> {
	let config_path = gopath::join(&[directory, "tsconfig.json"]);
	if extends.is_empty() {
		return Err(format!("{config_path}: extends path is empty"));
	}
	if !gopath::is_abs(extends) && !extends.starts_with('.') {
		let host = ResolveConfigHost {
			current_directory: gopath::to_slash(directory).into_owned(),
		};
		let resolved = tsrs_module::resolve_config(&gopath::to_slash(extends), &gopath::to_slash(&config_path), &host);
		if !resolved.is_resolved() {
			return Err(format!("{config_path}: resolve extends {extends:?}: config not found"));
		}
		return Ok(gopath::clean(&gopath::from_slash(resolved.resolved_file_name)));
	}
	let mut path = if gopath::is_abs(extends) {
		extends.to_owned()
	} else {
		gopath::join(&[directory, &gopath::from_slash(extends)])
	};
	match std::fs::metadata(&path) {
		Ok(metadata) if metadata.is_file() => return Ok(gopath::clean(&path)),
		Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
			return Err(format!(
				"{config_path}: resolve extends {extends:?}: stat {path}: {error}"
			));
		}
		_ => {}
	}
	if !path.ends_with(".json") {
		path.push_str(".json");
	}
	match std::fs::metadata(&path) {
		Ok(_) => Ok(gopath::clean(&path)),
		Err(error) => Err(format!(
			"{config_path}: resolve extends {extends:?}: stat {path}: {error}"
		)),
	}
}

struct ResolveConfigHost {
	current_directory: String,
}

impl tsrs_module::ResolutionHost for ResolveConfigHost {
	fn fs(&self) -> &dyn tsrs_vfs::FS {
		tsrs_vfs::osvfs::fs()
	}

	fn get_current_directory(&self) -> &str {
		&self.current_directory
	}
}

/// `path.resolve(base, p)` in Go's terms: an absolute `p` cleaned, otherwise joined onto `base`.
fn resolve_against(base: &str, path: &str) -> String {
	let path = gopath::from_slash(path);
	if gopath::is_abs(&path) {
		return gopath::clean(&path);
	}
	gopath::join(&[base, &path])
}

fn path_exists(path: &str) -> bool {
	StdPath::new(path).exists()
}

fn type_roots_contain(type_roots: &[String], project_path: &str, rbxts_modules: &str) -> bool {
	let want = gopath::to_slash(&gopath::clean(rbxts_modules)).into_owned();
	type_roots.iter().any(|type_root| {
		let resolved = resolve_against(&gopath::from_slash(project_path), type_root);
		gopath::to_slash(&gopath::clean(&resolved)) == want || path_exists(&resolved)
	})
}

/// Checks the parsed options against rbxtsc's required settings (`validateCompilerOptions`):
/// `None` when they pass, otherwise the full "Invalid tsconfig.json configuration!" message.
///
/// `project_path` is the absolute slash-separated project directory; `raw` carries the values the
/// sanitizer rewrote ([`read_raw_enforced_options`]).
#[must_use]
pub fn validate_compiler_options(
	options: &CompilerOptions,
	project_path: &str,
	raw: &RawEnforcedOptions,
) -> Option<String> {
	let mut errors: Vec<String> = Vec::new();
	if options.no_lib != Tristate::True {
		errors.push(r#""noLib" must be true"#.to_owned());
	}
	if options.strict != Tristate::True {
		errors.push(r#""strict" must be true"#.to_owned());
	}
	let is_common_js = options.module == ModuleKind::COMMON_JS;
	let is_preserve = options.module == ModuleKind::PRESERVE;
	if !is_common_js && !is_preserve {
		errors.push(r#""module" must be commonjs"#.to_owned());
	}
	if options.module_detection != ModuleDetectionKind::Force {
		errors.push(r#""moduleDetection" must be "force""#.to_owned());
	}
	let node10 = raw.has_module_resolution
		&& !is_preserve
		&& raw
			.module_resolution
			.as_deref()
			.is_some_and(is_node10_module_resolution_text);
	let bundler = is_preserve && options.get_module_resolution_kind() == ModuleResolutionKind::BUNDLER;
	if !node10 && !bundler {
		errors.push(r#""moduleResolution" must be "Node""#.to_owned());
	}
	if options.allow_synthetic_default_imports != Tristate::True {
		errors.push(r#""allowSyntheticDefaultImports" must be true"#.to_owned());
	}

	let rbxts_modules = gopath::join(&[&gopath::from_slash(project_path), "node_modules", "@rbxts"]);
	if !options
		.type_roots
		.as_deref()
		.is_some_and(|type_roots| type_roots_contain(type_roots, project_path, &rbxts_modules))
	{
		errors.push(format!(r#""typeRoots" must contain {rbxts_modules}"#));
	}
	let fallback = ["node_modules/@rbxts".to_owned()];
	let type_roots = options.type_roots.as_deref().unwrap_or(&fallback);
	for types_location in &raw.types {
		let found = type_roots.iter().any(|type_root| {
			let root = resolve_against(&gopath::from_slash(project_path), type_root);
			let types_path = resolve_against(&root, types_location);
			path_exists(&types_path) || path_exists(&format!("{types_path}.d.ts"))
		});
		if !found {
			errors.push(format!(
				r#""types" {types_location} were not found. Make sure the path is relative to `typeRoots`"#
			));
		}
	}

	if options.root_dir.is_empty() && options.root_dirs.is_none() {
		errors.push(r#""rootDir" or "rootDirs" must be defined"#.to_owned());
	}
	if options.out_dir.is_empty() {
		errors.push(r#""outDir" must be defined"#.to_owned());
	}
	if raw.has_imports_not_used {
		let suggested = if raw
			.imports_not_used
			.as_deref()
			.is_some_and(|value| value.eq_ignore_ascii_case("preserve"))
		{
			"true"
		} else {
			"false"
		};
		errors.push(format!(
			r#""importsNotUsedAsValues" is no longer supported, use "verbatimModuleSyntax": {suggested} instead"#
		));
	}

	if errors.is_empty() {
		return None;
	}
	let mut message = String::from(
		"Invalid \"tsconfig.json\" configuration!\nhttps://roblox-ts.com/docs/quick-start#project-folder-setup\n",
	);
	for error in errors {
		message.push_str("- ");
		message.push_str(&error);
		message.push('\n');
	}
	Some(message)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rewrites_only_the_declaration_form() {
		let source = "interface Iterable<T> {}\ninterface AsyncIterableIterator<T> extends Iterable<T> {}\n";
		assert_eq!(
			rewrite_iterable_arity(source),
			"interface Iterable<T, TReturn = any, TNext = any> {}\n\
			 interface AsyncIterableIterator<T, TReturn = any, TNext = any> extends Iterable<T> {}\n"
		);
		assert_eq!(
			rewrite_iterable_arity("xinterface Iterable<T>"),
			"xinterface Iterable<T>"
		);
	}

	#[test]
	fn strips_comments_and_trailing_commas_outside_strings() {
		let source = "{\n\t// note\n\t\"a\": \"//x,\", /* b */\n\t\"c\": [1, 2,],\n}";
		let stripped: Value = serde_json::from_str(&strip_jsonc(source)).expect("stripped JSON parses");
		assert_eq!(stripped, serde_json::json!({ "a": "//x,", "c": [1, 2] }));
	}

	#[test]
	fn sanitizes_the_removed_options() {
		let source = r#"{ "compilerOptions": { "downlevelIteration": true, "importsNotUsedAsValues": "remove",
			"moduleResolution": "Node", "baseUrl": "src", "strict": true } }"#;
		let sanitized: Value = serde_json::from_str(&sanitize_tsconfig(source)).expect("sanitized JSON parses");
		assert_eq!(
			sanitized,
			serde_json::json!({ "compilerOptions": {
				"moduleResolution": "bundler", "strict": true, "paths": { "*": ["./src/*"] }
			} })
		);
	}

	#[test]
	fn rebases_existing_paths_onto_the_base_url() {
		let source =
			r#"{ "compilerOptions": { "baseUrl": "./src/", "paths": { "@x/*": ["lib/*"], "*": ["./a/*"] } } }"#;
		let sanitized: Value = serde_json::from_str(&sanitize_tsconfig(source)).expect("sanitized JSON parses");
		assert_eq!(
			sanitized["compilerOptions"]["paths"],
			serde_json::json!({ "@x/*": ["./src/lib/*", "./src/@x/*"], "*": ["./src/a/*", "./src/*"] })
		);
	}

	#[test]
	fn lone_surrogates_decode_to_the_replacement_character() {
		let parsed = parse_json(r#"{ "a": "\ud800x", "b": "\udc00", "c": "\ud83d\ude00", "d": "\\ud800" }"#)
			.expect("lone surrogates parse");
		assert_eq!(
			parsed,
			serde_json::json!({ "a": "\u{fffd}x", "b": "\u{fffd}", "c": "\u{1f600}", "d": "\\ud800" })
		);
	}

	#[test]
	fn extends_reports_stat_errors_other_than_not_found() {
		let directory = tempfile::tempdir().expect("temporary directory");
		let root = directory.path().to_str().expect("UTF-8 path");
		std::fs::write(directory.path().join("base.json"), "{}").expect("write base");
		assert!(resolve_extends_path(root, "./base").is_ok_and(|path| path.ends_with("base.json")));
		assert!(resolve_extends_path(root, "./missing").is_err_and(|error| error.contains("missing.json")));
		// A file used as a directory fails with ENOTDIR, not "not found". Windows has no such error:
		// the path is not found, and the error names the `.json` it tried last.
		let want = if cfg!(windows) { "inner.json" } else { "directory" };
		assert!(resolve_extends_path(root, "./base.json/inner").is_err_and(|error| error.contains(want)));
		assert!(resolve_extends_path(root, "").is_err());
	}

	#[test]
	fn leaves_malformed_configs_alone() {
		assert_eq!(sanitize_tsconfig("{ nope"), "{ nope");
		assert_eq!(sanitize_tsconfig("[1]"), "[1]");
	}

	#[test]
	fn reports_every_rbxtsc_requirement() {
		let message = validate_compiler_options(&CompilerOptions::default(), "/p", &RawEnforcedOptions::default())
			.expect("default options fail validation");
		for needle in [
			"\"noLib\" must be true",
			"\"strict\" must be true",
			"\"module\" must be commonjs",
			"\"moduleDetection\" must be \"force\"",
			"\"moduleResolution\" must be \"Node\"",
			"\"allowSyntheticDefaultImports\" must be true",
			"\"rootDir\" or \"rootDirs\" must be defined",
			"\"outDir\" must be defined",
		] {
			assert!(message.contains(needle), "missing {needle} in {message}");
		}
		assert!(message.starts_with("Invalid \"tsconfig.json\" configuration!\n"));
	}
}
