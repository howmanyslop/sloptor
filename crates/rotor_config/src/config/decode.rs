//! Decodes `rotor.toml` into a [`Config`], following the behavior of the
//! `github.com/BurntSushi/toml` decoder the Go implementation uses: keys match
//! struct fields exactly or case-insensitively, keys that match nothing are
//! reported as undecoded, and type mismatches fail with its message shapes.

use std::collections::HashSet;
use std::fmt::Write;
use std::ops::Range;

use rotor_support::gostr::quote;
use toml_edit::{Document, Item, TableLike, TomlError, Value};

use super::flamework::{FlameworkConfig, FlameworkOptimizations, FlameworkProfile};
use super::model::Config;
use super::parallel_transformers::{
	ParallelTransformerDependency, ParallelTransformerPlugin, ParallelTransformersConfig,
};

type Res<T> = Result<T, String>;

/// A decoded document: the config plus every key that matched no field.
pub struct Decoded {
	/// The decoded settings.
	pub config: Config,
	/// Keys that decoded into nothing, in document order.
	pub undecoded: Vec<Vec<String>>,
}

/// The TOML data model as `BurntSushi`'s decoder sees it (`map[string]any`).
enum Data {
	Bool(bool),
	Int(i64),
	Float,
	Str(String),
	Datetime(&'static str),
	Table(Vec<Entry>),
	Array(Vec<Self>),
	TableArray(Vec<Self>),
}

struct Entry {
	key: String,
	line: usize,
	data: Data,
}

impl Data {
	/// Go's `%T` of the value `BurntSushi` would hold, as `fmtType` prints it.
	const fn type_name(&self) -> &'static str {
		match self {
			Self::Bool(_) => "bool",
			Self::Int(_) => "int64",
			Self::Float => "float64",
			Self::Str(_) => "string",
			Self::Datetime(name) => name,
			Self::Table(_) => "map[string]any",
			Self::Array(_) => "[]any",
			Self::TableArray(_) => "[]map[string]any",
		}
	}
}

/// `BurntSushi`'s `Key.String()`: dotted, with parts that are not bare keys
/// double-quoted.
pub fn key_string(path: &[String]) -> String {
	let mut out = String::new();
	for (i, part) in path.iter().enumerate() {
		if i > 0 {
			out.push('.');
		}
		let bare = !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
		if bare {
			out.push_str(part);
		} else {
			out.push('"');
			for c in part.chars() {
				match c {
					'"' => out.push_str("\\\""),
					'\\' => out.push_str("\\\\"),
					'\x08' => out.push_str("\\b"),
					'\t' => out.push_str("\\t"),
					'\n' => out.push_str("\\n"),
					'\x0c' => out.push_str("\\f"),
					'\r' => out.push_str("\\r"),
					c if c < ' ' || c == '\x7f' => {
						let _ = write!(out, "\\u{:04x}", u32::from(c));
					}
					c => out.push(c),
				}
			}
			out.push('"');
		}
	}
	out
}

/// Decodes `source`.
///
/// # Errors
///
/// Returns `BurntSushi`-shaped text (`toml: line N (last key "k"): ...`) for a
/// type mismatch, and `toml: line N: <message>` for a syntax error. The
/// parser's own message differs from `BurntSushi`'s wording.
pub fn decode(source: &str) -> Res<Decoded> {
	let source = source.strip_prefix('\u{feff}').unwrap_or(source);
	let document = Document::parse(source).map_err(|err| syntax_error(source, &err))?;
	let root = document.as_table();

	let mut keys = Vec::new();
	collect_keys(root, &mut Vec::new(), &mut keys);
	keys.sort_by_key(|(position, _)| *position);

	let data = table_entries(root, source);
	let mut decoder = Decoder {
		decoded: HashSet::new(),
		context: Vec::new(),
	};
	let mut config = Config::default();
	decoder.config(&data, &mut config)?;

	let undecoded = keys
		.into_iter()
		.map(|(_, path)| path)
		.filter(|path| !decoder.decoded.contains(&key_string(path)))
		.collect();
	Ok(Decoded { config, undecoded })
}

fn line_of(source: &str, span: Option<Range<usize>>) -> usize {
	span.map_or(0, |span| {
		source[..span.start.min(source.len())].matches('\n').count() + 1
	})
}

fn syntax_error(source: &str, err: &TomlError) -> String {
	err.span().map_or_else(
		|| format!("toml: {}", err.message()),
		|span| format!("toml: line {}: {}", line_of(source, Some(span)), err.message()),
	)
}

/// Every key `BurntSushi` records in document order: table headers, array-table
/// headers, inline tables, and key/value pairs at their full dotted path.
/// Implicit parent tables and the dotted prefixes of a key do not count.
fn collect_keys(table: &dyn TableLike, prefix: &mut Vec<String>, out: &mut Vec<(usize, Vec<String>)>) {
	for (name, item) in table.iter() {
		prefix.push(name.to_owned());
		let key_position = table
			.get_key_value(name)
			.and_then(|(key, _)| key.span())
			.map_or(0, |span| span.start);
		match item {
			Item::Table(child) => {
				if !child.is_dotted() && !child.is_implicit() {
					out.push((child.span().map_or(key_position, |span| span.start), prefix.clone()));
				}
				collect_keys(child, prefix, out);
			}
			Item::ArrayOfTables(tables) => {
				for child in tables {
					out.push((child.span().map_or(key_position, |span| span.start), prefix.clone()));
					collect_keys(child, prefix, out);
				}
			}
			Item::Value(Value::InlineTable(child)) => {
				out.push((key_position, prefix.clone()));
				collect_keys(child, prefix, out);
			}
			Item::Value(Value::Array(array)) => {
				out.push((key_position, prefix.clone()));
				for element in array {
					if let Value::InlineTable(child) = element {
						collect_keys(child, prefix, out);
					}
				}
			}
			Item::Value(_) | Item::None => out.push((key_position, prefix.clone())),
		}
		prefix.pop();
	}
}

fn table_entries(table: &dyn TableLike, source: &str) -> Vec<Entry> {
	table
		.iter()
		.map(|(key, item)| Entry {
			key: key.to_owned(),
			line: line_of(source, table.get_key_value(key).and_then(|(k, _)| k.span())),
			data: item_data(item, source),
		})
		.collect()
}

fn item_data(item: &Item, source: &str) -> Data {
	match item {
		Item::Table(table) => Data::Table(table_entries(table, source)),
		Item::ArrayOfTables(tables) => Data::TableArray(
			tables
				.iter()
				.map(|table| Data::Table(table_entries(table, source)))
				.collect(),
		),
		Item::Value(value) => value_data(value, source),
		Item::None => Data::Table(Vec::new()),
	}
}

fn value_data(value: &Value, source: &str) -> Data {
	match value {
		Value::String(s) => Data::Str(s.value().clone()),
		Value::Integer(i) => Data::Int(*i.value()),
		Value::Float(_) => Data::Float,
		Value::Boolean(b) => Data::Bool(*b.value()),
		// BurntSushi decodes every datetime flavor into `time.Time`.
		Value::Datetime(_) => Data::Datetime("time.Time"),
		Value::Array(array) => Data::Array(array.iter().map(|element| value_data(element, source)).collect()),
		Value::InlineTable(table) => Data::Table(table_entries(table, source)),
	}
}

/// The decoder's key context, as `BurntSushi`'s `MetaData` keeps it.
struct Decoder {
	decoded: HashSet<String>,
	context: Vec<(String, usize)>,
}

impl Decoder {
	fn enter(&mut self, entry: &Entry) {
		self.context.push((entry.key.clone(), entry.line));
		self.decoded.insert(self.context_key());
	}

	fn leave(&mut self) {
		self.context.pop();
	}

	fn context_key(&self) -> String {
		let path: Vec<String> = self.context.iter().map(|(key, _)| key.clone()).collect();
		key_string(&path)
	}

	/// `md.e`: a message prefixed with the key being decoded and its line.
	fn error(&self, message: &str) -> String {
		match self.context.last() {
			None => format!("toml: {message}"),
			Some((_, 0)) => format!("toml: (last key {}): {message}", quote(&self.context_key())),
			Some((_, line)) => format!("toml: line {line} (last key {}): {message}", quote(&self.context_key())),
		}
	}

	fn bad_type(&self, destination: &str, data: &Data) -> String {
		self.error(&format!(
			"incompatible types: TOML value has type {}; destination has type {destination}",
			data.type_name()
		))
	}

	fn boolean(&self, data: &Data) -> Res<bool> {
		match data {
			Data::Bool(b) => Ok(*b),
			other => Err(self.bad_type("boolean", other)),
		}
	}

	fn string(&self, data: &Data) -> Res<String> {
		match data {
			Data::Str(s) => Ok(s.clone()),
			other => Err(self.bad_type("string", other)),
		}
	}

	fn integer(&self, data: &Data) -> Res<i64> {
		match data {
			Data::Int(n) => Ok(*n),
			other => Err(self.bad_type("integer", other)),
		}
	}

	/// The entries of a table decoded into the Go struct `go_type`.
	fn struct_entries<'a>(&self, data: &'a Data, go_type: &str) -> Res<&'a [Entry]> {
		match data {
			Data::Table(entries) => Ok(entries),
			other => Err(self.error(&format!(
				"type mismatch for {go_type}: expected table but found {}",
				other.type_name()
			))),
		}
	}

	fn slice<'a>(&self, data: &'a Data) -> Res<&'a [Data]> {
		match data {
			Data::Array(items) | Data::TableArray(items) => Ok(items),
			other => Err(self.bad_type("slice", other)),
		}
	}

	fn config(&mut self, entries: &[Entry], out: &mut Config) -> Res<()> {
		const FIELDS: [&str; 4] = ["parallelTransformers", "flamework", "nativeTypes", "luauTypes"];
		for entry in entries {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => self.parallel_transformers(&entry.data, out.parallel_transformers.get_or_insert_default())?,
				1 => self.flamework(&entry.data, out.flamework.get_or_insert_default())?,
				2 => out.native_types = Some(self.boolean(&entry.data)?),
				_ => out.luau_types = Some(self.boolean(&entry.data)?),
			}
			self.leave();
		}
		Ok(())
	}

	fn parallel_transformers(&mut self, data: &Data, out: &mut ParallelTransformersConfig) -> Res<()> {
		const FIELDS: [&str; 5] = ["workers", "batchIndependent", "typescript", "plugins", "dependencies"];
		for entry in self.struct_entries(data, "config.ParallelTransformersConfig")? {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => out.workers = self.integer(&entry.data)?,
				1 => out.batch_independent = self.boolean(&entry.data)?,
				2 => out.typescript = self.string(&entry.data)?,
				3 => {
					let mut plugins = Vec::new();
					for item in self.slice(&entry.data)? {
						let mut plugin = ParallelTransformerPlugin::default();
						self.plugin(item, &mut plugin)?;
						plugins.push(plugin);
					}
					out.plugins = plugins;
				}
				_ => {
					let mut dependencies = Vec::new();
					for item in self.slice(&entry.data)? {
						let mut dependency = ParallelTransformerDependency::default();
						self.dependency(item, &mut dependency)?;
						dependencies.push(dependency);
					}
					out.dependencies = dependencies;
				}
			}
			self.leave();
		}
		Ok(())
	}

	fn plugin(&mut self, data: &Data, out: &mut ParallelTransformerPlugin) -> Res<()> {
		const FIELDS: [&str; 2] = ["config", "sha256"];
		for entry in self.struct_entries(data, "config.ParallelTransformerPlugin")? {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => out.config = self.string(&entry.data)?,
				_ => out.sha256 = self.string(&entry.data)?,
			}
			self.leave();
		}
		Ok(())
	}

	fn dependency(&mut self, data: &Data, out: &mut ParallelTransformerDependency) -> Res<()> {
		const FIELDS: [&str; 3] = ["from", "specifier", "sha256"];
		for entry in self.struct_entries(data, "config.ParallelTransformerDependency")? {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => out.from = self.integer(&entry.data)?,
				1 => out.specifier = self.string(&entry.data)?,
				_ => out.sha256 = self.string(&entry.data)?,
			}
			self.leave();
		}
		Ok(())
	}

	fn flamework(&mut self, data: &Data, out: &mut FlameworkConfig) -> Res<()> {
		const FIELDS: [&str; 10] = [
			"after",
			"noSemanticDiagnostics",
			"obfuscation",
			"idGenerationMode",
			"hashPrefix",
			"salt",
			"preloadIds",
			"skipUnchangedFiles",
			"optimizations",
			"profiles",
		];
		for entry in self.struct_entries(data, "config.FlameworkConfig")? {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => out.after = self.string(&entry.data)?,
				1 => out.no_semantic_diagnostics = self.boolean(&entry.data)?,
				2 => out.obfuscation = self.boolean(&entry.data)?,
				3 => out.id_generation_mode = self.string(&entry.data)?,
				4 => out.hash_prefix = self.string(&entry.data)?,
				5 => out.salt = self.string(&entry.data)?,
				6 => out.preload_ids = self.boolean(&entry.data)?,
				7 => out.skip_unchanged_files = self.boolean(&entry.data)?,
				8 => self.optimizations(&entry.data, &mut out.optimizations)?,
				_ => self.profiles(&entry.data, out)?,
			}
			self.leave();
		}
		Ok(())
	}

	/// Decodes the `profiles` map. A value that is not a table is ignored
	/// without error, as `BurntSushi`'s `unifyMap` does.
	fn profiles(&mut self, data: &Data, out: &mut FlameworkConfig) -> Res<()> {
		let Data::Table(entries) = data else {
			return Ok(());
		};
		for entry in entries {
			self.enter(entry);
			let mut profile = FlameworkProfile::default();
			self.profile(&entry.data, &mut profile)?;
			out.profiles.insert(entry.key.clone(), profile);
			self.leave();
		}
		Ok(())
	}

	fn profile(&mut self, data: &Data, out: &mut FlameworkProfile) -> Res<()> {
		const FIELDS: [&str; 9] = [
			"after",
			"noSemanticDiagnostics",
			"obfuscation",
			"idGenerationMode",
			"hashPrefix",
			"salt",
			"preloadIds",
			"skipUnchangedFiles",
			"optimizations",
		];
		for entry in self.struct_entries(data, "config.FlameworkProfile")? {
			let Some(field) = match_field(&entry.key, &FIELDS) else {
				continue;
			};
			self.enter(entry);
			match field {
				0 => out.after = self.string(&entry.data)?,
				1 => out.no_semantic_diagnostics = self.boolean(&entry.data)?,
				2 => out.obfuscation = self.boolean(&entry.data)?,
				3 => out.id_generation_mode = self.string(&entry.data)?,
				4 => out.hash_prefix = self.string(&entry.data)?,
				5 => out.salt = self.string(&entry.data)?,
				6 => out.preload_ids = self.boolean(&entry.data)?,
				7 => out.skip_unchanged_files = self.boolean(&entry.data)?,
				_ => self.optimizations(&entry.data, &mut out.optimizations)?,
			}
			self.leave();
		}
		Ok(())
	}

	fn optimizations(&mut self, data: &Data, out: &mut FlameworkOptimizations) -> Res<()> {
		for entry in self.struct_entries(data, "config.FlameworkOptimizations")? {
			if match_field(&entry.key, &["guardGenerationDedupLimit"]).is_none() {
				continue;
			}
			self.enter(entry);
			out.guard_generation_dedup_limit = Some(self.integer(&entry.data)?);
			self.leave();
		}
		Ok(())
	}
}

/// The index of the field `key` decodes into: an exact name match, else the
/// first case-insensitive one.
fn match_field(key: &str, names: &[&str]) -> Option<usize> {
	names
		.iter()
		.position(|name| *name == key)
		.or_else(|| names.iter().position(|name| name.to_lowercase() == key.to_lowercase()))
}

#[cfg(test)]
mod tests;
