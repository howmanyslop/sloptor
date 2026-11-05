//! `transformer_host.go`: the native-to-JavaScript transformer protocol, shared by the Node
//! sidecar and API callback hosts, and the request each call sends.

use std::collections::BTreeMap;
use std::sync::Arc;

use rotor_config::config::ParallelTransformersConfig;
use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use serde_json::Value;

use crate::compile::Error;
use crate::gojson;
use crate::incremental_manifest::normalize_source_file_path;
use crate::project::{Cancellation, lock};
use crate::sidecar::{SidecarBuildState, SidecarCallStats, SidecarSession, canonical_sidecar_path};
use crate::sidecar_parallel::CliTransformerBuild;
use crate::sidecar_roots::narrowed_sidecar_roots;

/// `TransformerRequest`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerRequest {
	/// Always 1.
	pub protocol: i64,
	/// `transform`, `validate`, `warm` or `parallel-preflight`.
	pub operation: String,
	/// The config, in the worker's physical spelling.
	pub ts_config_path: String,
	/// The project directory, in the worker's physical spelling.
	pub project_dir: String,
	/// The files to transform; `None` is Go's nil slice.
	pub compile_file_names: Option<Vec<String>>,
	/// The narrowed root set; empty leaves the worker's roots alone.
	pub root_file_names: Vec<String>,
	/// The texts the worker must use instead of disk; `None` is Go's nil slice.
	pub changed_files: Option<Vec<TransformerChangedFile>>,
	/// The plugin entries as raw JSON; empty runs the config's own.
	pub plugins: Vec<String>,
	/// A parallel split's attestation.
	pub parallel_attestation: Option<ParallelTransformersConfig>,
	/// Whether this is one worker of a parallel split.
	pub parallel_worker: bool,
	/// `original-authored` when the files are the authored source.
	pub source_origin: String,
}

/// `TransformerChangedFile`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerChangedFile {
	/// The file.
	pub file_name: String,
	/// Its text.
	pub text: String,
	/// Native jecs's rewrite of the file on disk.
	pub rebase: Option<Arc<TransformerRebase>>,
}

/// `TransformerRebase` (`jecs_rebase.go`): the file on disk a rewrite's ranges index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerRebase {
	/// The file on disk.
	pub text: String,
	/// Five numbers per node of the printed rewrite.
	pub ranges: Vec<i64>,
}

/// `TransformerResponse`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerResponse {
	/// The diagnostics.
	pub diagnostics: Vec<TransformerDiagnostic>,
	/// The transformed files.
	pub transformed: Vec<TransformerOutputFile>,
	/// The worker's metrics.
	pub metrics: Option<TransformerResponseMetrics>,
	/// How many `afterDeclarations` transformers the config has.
	pub after_declarations_transformers: i64,
	/// The bytes a callback transport measured.
	pub transport: Option<TransformerTransportMetrics>,
	/// A parallel preflight's answer.
	pub parallel_ready: bool,
	/// A parallel worker's shared batch diagnostics.
	pub parallel_batch_diagnostics: i64,
	/// A parallel worker's missing-source diagnostics.
	pub parallel_source_diagnostics: i64,
}

/// `TransformerTransportMetrics`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransformerTransportMetrics {
	/// The bytes sent.
	pub request_bytes: i64,
	/// The bytes received.
	pub response_bytes: i64,
}

/// `TransformerResponseMetrics`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerResponseMetrics {
	/// The worker's wall time.
	pub wall_ms: i64,
	/// The worker's user CPU.
	pub cpu_user_us: i64,
	/// The worker's system CPU.
	pub cpu_system_us: i64,
	/// The worker's Node version.
	pub node_version: String,
	/// Each plugin's share.
	pub plugins: Vec<TransformerPluginMetric>,
	/// The request's phases.
	pub phases: Option<TransformerPhaseMetrics>,
}

/// `TransformerPhaseMetrics`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(missing_docs, reason = "one field per phase, named as in the protocol")]
pub struct TransformerPhaseMetrics {
	pub config_ms: i64,
	pub program_ms: i64,
	pub factories_ms: i64,
	pub transform_ms: i64,
	pub parent_repair_ms: i64,
	pub print_ms: i64,
}

/// `TransformerPluginMetric`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerPluginMetric {
	/// The plugin's `transform`.
	pub transform: String,
	/// The time inside its factory and visits.
	pub ms: i64,
}

/// `TransformerDiagnostic`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerDiagnostic {
	/// `error`, `warning`, ...
	pub category: String,
	/// The code.
	pub code: String,
	/// The file.
	pub file: String,
	/// The start offset.
	pub start: i64,
	/// The length.
	pub length: i64,
	/// The message.
	pub message: String,
}

/// `TransformerOutputFile`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformerOutputFile {
	/// The file.
	pub file_name: String,
	/// The transformed text.
	pub text: String,
	/// The trace back to the input text.
	pub trace_map: String,
}

impl TransformerRequest {
	/// The request as Go's `json.Marshal` writes it.
	#[must_use]
	pub fn to_json(&self) -> String {
		let mut object = gojson::Object::new()
			.int("protocol", self.protocol)
			.string("operation", &self.operation)
			.string("tsConfigPath", &self.ts_config_path)
			.string("projectDir", &self.project_dir)
			.raw(
				"compileFileNames",
				&self
					.compile_file_names
					.as_ref()
					.map_or_else(|| "null".to_owned(), |names| gojson::string_array(names)),
			);
		if !self.root_file_names.is_empty() {
			object = object.raw("rootFileNames", &gojson::string_array(&self.root_file_names));
		}
		object = object.raw(
			"changedFiles",
			&self.changed_files.as_ref().map_or_else(
				|| "null".to_owned(),
				|files| gojson::array(files.iter().map(TransformerChangedFile::to_json)),
			),
		);
		if !self.plugins.is_empty() {
			object = object.raw("plugins", &gojson::array(self.plugins.iter().cloned()));
		}
		if let Some(attestation) = &self.parallel_attestation {
			object = object.raw("parallelAttestation", &parallel_attestation_json(attestation));
		}
		if self.parallel_worker {
			object = object.bool("parallelWorker", true);
		}
		object.string_omitempty("sourceOrigin", &self.source_origin).finish()
	}
}

impl TransformerChangedFile {
	fn to_json(&self) -> String {
		let object = gojson::Object::new()
			.string("fileName", &self.file_name)
			.string("text", &self.text);
		match &self.rebase {
			None => object.finish(),
			Some(rebase) => object
				.raw(
					"rebase",
					&gojson::Object::new()
						.string("text", &rebase.text)
						.raw("ranges", &gojson::array(rebase.ranges.iter().map(ToString::to_string)))
						.finish(),
				)
				.finish(),
		}
	}
}

fn parallel_attestation_json(config: &ParallelTransformersConfig) -> String {
	gojson::Object::new()
		.int("workers", config.workers)
		.bool("batchIndependent", config.batch_independent)
		.string("typescript", &config.typescript)
		.raw(
			"plugins",
			&gojson::array(config.plugins.iter().map(|plugin| {
				gojson::Object::new()
					.string("config", &plugin.config)
					.string("sha256", &plugin.sha256)
					.finish()
			})),
		)
		.raw(
			"dependencies",
			&gojson::array(config.dependencies.iter().map(|dependency| {
				gojson::Object::new()
					.int("from", dependency.from)
					.string("specifier", &dependency.specifier)
					.string("sha256", &dependency.sha256)
					.finish()
			})),
		)
		.finish()
}

/// Go's `json.Unmarshal` of one field: a missing key or `null` leaves the zero value, a value of
/// another type is an error.
fn field<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
	object
		.get(key)
		.or_else(|| {
			object
				.iter()
				.rev()
				.find(|(name, _)| name.eq_ignore_ascii_case(key))
				.map(|(_, value)| value)
		})
		.filter(|value| !value.is_null())
}

fn type_error(kind: &str, value: &Value, path: &str) -> String {
	let found = match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	};
	format!("json: cannot unmarshal {found} into Go struct field {path} of type {kind}")
}

fn int_field(object: &serde_json::Map<String, Value>, key: &str, path: &str) -> Result<i64, String> {
	field(object, key).map_or(Ok(0), |value| {
		value.as_i64().ok_or_else(|| type_error("int", value, path))
	})
}

fn string_field(object: &serde_json::Map<String, Value>, key: &str, path: &str) -> Result<String, String> {
	match field(object, key) {
		None => Ok(String::new()),
		Some(Value::String(text)) => Ok(text.clone()),
		Some(value) => Err(type_error("string", value, path)),
	}
}

/// The key [`field`] reads: the exact one, else the last case-insensitive match.
fn field_key(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
	if object.contains_key(key) {
		return Some(key.to_owned());
	}
	object.keys().rev().find(|name| name.eq_ignore_ascii_case(key)).cloned()
}

/// [`string_field`] that moves the string out of the decoded value instead of copying it.
fn take_string_field(object: &mut serde_json::Map<String, Value>, key: &str, path: &str) -> Result<String, String> {
	let Some(name) = field_key(object, key) else {
		return Ok(String::new());
	};
	match object.get_mut(&name) {
		None | Some(Value::Null) => Ok(String::new()),
		Some(Value::String(text)) => Ok(std::mem::take(text)),
		Some(value) => Err(type_error("string", value, path)),
	}
}

/// [`array_field`] over a value the decode may take strings out of.
fn take_array_field(object: &mut serde_json::Map<String, Value>, key: &str, path: &str) -> Result<Vec<Value>, String> {
	let Some(name) = field_key(object, key) else {
		return Ok(Vec::new());
	};
	match object.get_mut(&name) {
		None | Some(Value::Null) => Ok(Vec::new()),
		Some(Value::Array(values)) => Ok(std::mem::take(values)),
		Some(value) => Err(type_error("[]", value, path)),
	}
}

fn bool_field(object: &serde_json::Map<String, Value>, key: &str, path: &str) -> Result<bool, String> {
	match field(object, key) {
		None => Ok(false),
		Some(Value::Bool(flag)) => Ok(*flag),
		Some(value) => Err(type_error("bool", value, path)),
	}
}

fn array_field<'a>(object: &'a serde_json::Map<String, Value>, key: &str, path: &str) -> Result<&'a [Value], String> {
	match field(object, key) {
		None => Ok(&[]),
		Some(Value::Array(values)) => Ok(values),
		Some(value) => Err(type_error("[]", value, path)),
	}
}

fn object_of<'a>(value: &'a Value, path: &str) -> Result<&'a serde_json::Map<String, Value>, String> {
	value.as_object().ok_or_else(|| type_error("struct", value, path))
}

impl TransformerResponse {
	/// Decodes a worker's response line (`json.Unmarshal`).
	///
	/// # Errors
	///
	/// The line is not JSON or a field has the wrong type.
	pub fn from_json(text: &str) -> Result<Self, String> {
		let mut value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
		let Value::Object(object) = &mut value else {
			if value.is_null() {
				return Ok(Self::default());
			}
			return Err(type_error("compile.TransformerResponse", &value, "TransformerResponse"));
		};
		let diagnostics = take_array_field(object, "diagnostics", "TransformerResponse.diagnostics")?
			.into_iter()
			.map(|mut value| {
				let object = match &mut value {
					Value::Object(object) => object,
					value => return Err(type_error("struct", value, "TransformerResponse.diagnostics")),
				};
				Ok(TransformerDiagnostic {
					category: take_string_field(object, "category", "TransformerResponse.diagnostics.category")?,
					code: take_string_field(object, "code", "TransformerResponse.diagnostics.code")?,
					file: take_string_field(object, "file", "TransformerResponse.diagnostics.file")?,
					start: int_field(object, "start", "TransformerResponse.diagnostics.start")?,
					length: int_field(object, "length", "TransformerResponse.diagnostics.length")?,
					message: take_string_field(object, "message", "TransformerResponse.diagnostics.message")?,
				})
			})
			.collect::<Result<Vec<_>, String>>()?;
		let transformed = take_array_field(object, "transformed", "TransformerResponse.transformed")?
			.into_iter()
			.map(|mut value| {
				let object = match &mut value {
					Value::Object(object) => object,
					value => return Err(type_error("struct", value, "TransformerResponse.transformed")),
				};
				Ok(TransformerOutputFile {
					file_name: take_string_field(object, "fileName", "TransformerResponse.transformed.fileName")?,
					text: take_string_field(object, "text", "TransformerResponse.transformed.text")?,
					trace_map: take_string_field(object, "traceMap", "TransformerResponse.transformed.traceMap")?,
				})
			})
			.collect::<Result<Vec<_>, String>>()?;
		let metrics = match field(object, "metrics") {
			None => None,
			Some(value) => Some(decode_metrics(object_of(value, "TransformerResponse.metrics")?)?),
		};
		Ok(Self {
			diagnostics,
			transformed,
			metrics,
			after_declarations_transformers: int_field(
				object,
				"afterDeclarationsTransformers",
				"TransformerResponse.afterDeclarationsTransformers",
			)?,
			transport: None,
			parallel_ready: bool_field(object, "parallelReady", "TransformerResponse.parallelReady")?,
			parallel_batch_diagnostics: int_field(
				object,
				"parallelBatchDiagnostics",
				"TransformerResponse.parallelBatchDiagnostics",
			)?,
			parallel_source_diagnostics: int_field(
				object,
				"parallelSourceDiagnostics",
				"TransformerResponse.parallelSourceDiagnostics",
			)?,
		})
	}
}

fn decode_metrics(object: &serde_json::Map<String, Value>) -> Result<TransformerResponseMetrics, String> {
	let path = "TransformerResponse.metrics";
	let plugins = array_field(object, "plugins", &format!("{path}.plugins"))?
		.iter()
		.map(|value| {
			let plugin = object_of(value, &format!("{path}.plugins"))?;
			Ok(TransformerPluginMetric {
				transform: string_field(plugin, "transform", &format!("{path}.plugins.transform"))?,
				ms: int_field(plugin, "ms", &format!("{path}.plugins.ms"))?,
			})
		})
		.collect::<Result<Vec<_>, String>>()?;
	let phases = match field(object, "phases") {
		None => None,
		Some(value) => {
			let phases = object_of(value, &format!("{path}.phases"))?;
			let phase = |key: &str| int_field(phases, key, &format!("{path}.phases.{key}"));
			Some(TransformerPhaseMetrics {
				config_ms: phase("configMs")?,
				program_ms: phase("programMs")?,
				factories_ms: phase("factoriesMs")?,
				transform_ms: phase("transformMs")?,
				parent_repair_ms: phase("parentRepairMs")?,
				print_ms: phase("printMs")?,
			})
		}
	};
	Ok(TransformerResponseMetrics {
		wall_ms: int_field(object, "wallMs", &format!("{path}.wallMs"))?,
		cpu_user_us: int_field(object, "cpuUserUs", &format!("{path}.cpuUserUs"))?,
		cpu_system_us: int_field(object, "cpuSystemUs", &format!("{path}.cpuSystemUs"))?,
		node_version: string_field(object, "nodeVersion", &format!("{path}.nodeVersion"))?,
		plugins,
		phases,
	})
}

/// `TransformerCallback`: runs a transformer request in an API client.
pub type TransformerCallback =
	Arc<dyn Fn(Option<&Cancellation>, TransformerRequest) -> Result<TransformerResponse, Error> + Send + Sync>;

/// `transformerHost`: where transformer requests run.
pub(crate) trait TransformerHost: Sync {
	/// `transform`
	fn transform(
		&self,
		cancel: Option<&Cancellation>,
		call: TransformerCall,
	) -> Result<(TransformerResponse, SidecarCallStats), Error>;
	/// `validate`
	fn validate(
		&self,
		cancel: Option<&Cancellation>,
		dir: &str,
		config_path: &str,
	) -> Result<(TransformerResponse, SidecarCallStats), Error>;
	/// Whether this host is the Node subprocess (Go's `usesSubprocessSidecar`).
	fn uses_subprocess_sidecar(&self) -> bool {
		false
	}
}

/// `transformerCall`
#[derive(Clone, Default)]
pub(crate) struct TransformerCall {
	pub(crate) dir: String,
	pub(crate) config_path: String,
	pub(crate) compile_files: Vec<P<SourceFile>>,
	pub(crate) stamp_files: Vec<P<SourceFile>>,
	pub(crate) overlays: BTreeMap<String, String>,
	/// `None` is Go's nil plugin list: the worker runs the config's own.
	pub(crate) plugins: Option<Vec<String>>,
	pub(crate) state: Option<Arc<std::sync::Mutex<SidecarBuildState>>>,
	pub(crate) parallel: Option<ParallelTransformersConfig>,
	pub(crate) parallel_build: Option<Arc<CliTransformerBuild>>,
	pub(crate) source_origin: String,
}

/// `transformerOriginalAuthored`
pub(crate) const TRANSFORMER_ORIGINAL_AUTHORED: &str = "original-authored";

/// `canonicalTransformerRequestPaths`: the physical directory and config, and the slot key.
pub(crate) fn canonical_transformer_request_paths(dir: &str, config_path: &str) -> (String, String, String) {
	let dir = canonical_sidecar_path(dir);
	let config_path = canonical_sidecar_path(config_path);
	let key = format!("{dir}|{config_path}");
	(dir, config_path, key)
}

/// `prepareTransformerRequest`
pub(crate) fn prepare_transformer_request(
	call: &TransformerCall,
	tracker: &mut SidecarSession,
	dir: &str,
	config_path: &str,
	stats: &mut SidecarCallStats,
) -> Result<TransformerRequest, Error> {
	let stamp_names: Vec<String> = call
		.stamp_files
		.iter()
		.map(|file| file.file_name().to_owned())
		.collect();
	let build_state = call.state.as_ref().map(|state| lock(state).clone());
	let (overlays, overlay_reads) =
		crate::sidecar::merge_sidecar_overlays(&call.compile_files, &call.overlays, build_state.as_ref(), true);
	stats.reads += overlay_reads;
	let skip_disk_scan = build_state.as_ref().is_some_and(|state| state.disk_scanned) && !tracker.stamps.is_empty();
	let (changed_files, io_stats) = tracker.collect_changed_files(&stamp_names, &overlays, skip_disk_scan);
	stats.stats += io_stats.stats;
	stats.reads += io_stats.reads;
	stats.changed_files += io_stats.changed_files;
	let mut changed_files = changed_files?;
	if let Some(state) = &build_state
		&& !state.rebases.is_empty()
	{
		for changed in &mut changed_files {
			changed.rebase = state
				.rebases
				.get(&normalize_source_file_path(&gopath::to_slash(&changed.file_name)))
				.cloned();
		}
	}
	let mut request = TransformerRequest {
		protocol: 1,
		operation: "transform".to_owned(),
		ts_config_path: gopath::from_slash(config_path).into_owned(),
		project_dir: gopath::from_slash(dir).into_owned(),
		compile_file_names: Some(
			call.compile_files
				.iter()
				.map(|file| gopath::from_slash(file.file_name()).into_owned())
				.collect(),
		),
		changed_files: Some(changed_files),
		plugins: call.plugins.clone().unwrap_or_default(),
		..TransformerRequest::default()
	};
	request.root_file_names = narrowed_sidecar_roots(&call.compile_files, &call.stamp_files);
	if crate::sidecar::getenv("ROTOR_REACT_SYNTAX_HOST").is_some_and(|value| value == "1")
		&& call.source_origin == TRANSFORMER_ORIGINAL_AUTHORED
	{
		TRANSFORMER_ORIGINAL_AUTHORED.clone_into(&mut request.source_origin);
	}
	Ok(request)
}

/// `prepareTransformerValidationRequest`: the request, the physical directory and config, and
/// the slot key.
pub(crate) fn prepare_transformer_validation_request(
	dir: &str,
	config_path: &str,
) -> (TransformerRequest, String, String, String) {
	let (dir, config_path, key) = canonical_transformer_request_paths(dir, config_path);
	(
		TransformerRequest {
			protocol: 1,
			operation: "validate".to_owned(),
			ts_config_path: gopath::from_slash(&config_path).into_owned(),
			project_dir: gopath::from_slash(&dir).into_owned(),
			..TransformerRequest::default()
		},
		dir,
		config_path,
		key,
	)
}

/// `WithTransformerCallback`: runs this build's transformer requests through `callback`.
pub fn with_transformer_callback(opts: &mut crate::project::ProjectOptions, callback: TransformerCallback) {
	opts.transformer_callback = Some(Arc::new(crate::transformer_host_sidecar::CallbackTransformerHost::new(
		callback,
	)));
}

/// `WithTransformerCallback`'s host, made once per API session as Go's is.
///
/// The build API makes it at `initialize`. Each project's tracker keeps the stamps the client's
/// worker has seen, so a later build sends the files changed since. A host made per build would
/// start every build fresh and send none, and the client would transform the text it kept from
/// before the edit.
#[derive(Clone)]
pub struct TransformerCallbackHost(Arc<crate::transformer_host_sidecar::CallbackTransformerHost>);

impl TransformerCallbackHost {
	/// `newCallbackTransformerHost`
	#[must_use]
	pub fn new(callback: TransformerCallback) -> Self {
		Self(Arc::new(crate::transformer_host_sidecar::CallbackTransformerHost::new(
			callback,
		)))
	}
}

/// Runs this build's transformer requests through the session's `host`.
pub fn with_transformer_host(opts: &mut crate::project::ProjectOptions, host: &TransformerCallbackHost) {
	opts.transformer_callback = Some(Arc::clone(&host.0));
}

/// `TransformerCallbackFromContext`: whether this build runs transformers through a callback.
#[must_use]
pub const fn has_transformer_callback(opts: &crate::project::ProjectOptions) -> bool {
	opts.transformer_callback.is_some()
}
