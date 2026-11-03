//! `package_layout.go`: one build's package import inputs, shared by the incremental salt and
//! emission. It must not survive into the next build.

use std::fmt::Write as _;

use rotor_rojo::{ProjectFiles, RojoResolver};
use rotor_support::gopath;
use rotor_transformer::{NodeModulesPathMapping, ProjectType};
use sha2::{Digest, Sha256};

use crate::compile::Error;
use crate::gojson;
use crate::incremental_salt::portable_incremental_rojo_state;
use crate::packageexports::{build_node_modules_path_mappings, remap_runtime_path};
use crate::project::{ProjectOptions, project_is_package};

/// `packageLayout`
pub struct PackageLayout {
	/// One package import resolver per type root.
	pub resolvers: Vec<RojoResolver>,
	/// Types entry to runtime path, canonical keys and realpath aliases included.
	pub mapping: NodeModulesPathMapping,
	/// The same mapping with authored spellings, for fingerprints.
	pub portable_mapping: NodeModulesPathMapping,
	/// The package project files the resolvers read.
	pub files: ProjectFiles,
	/// Every `package.json` the mapping read.
	pub manifests: Vec<String>,
}

/// `isPackageBuild`: the `--type` override, else the scoped `package.json` name.
pub(crate) fn is_package_build(dir: &str, opts: &ProjectOptions) -> Result<bool, Error> {
	if let Some(project_type) = opts.project_type {
		return Ok(project_type == ProjectType::Package);
	}
	project_is_package(dir).map(|(_, is_package)| is_package)
}

/// `preparePackageLayout`: `None` unless this is a package build.
pub(crate) fn prepare_package_layout(
	dir: &str,
	options: &rotor_tsrs::CompilerOptions,
	opts: &ProjectOptions,
) -> Result<Option<PackageLayout>, Error> {
	if opts.emit_declaration_only || !is_package_build(dir, opts)? {
		return Ok(None);
	}
	let files = ProjectFiles::new();
	let type_roots = options.type_roots.clone().unwrap_or_default();
	let resolvers = type_roots
		.iter()
		.map(|type_root| RojoResolver::package_imports(&gopath::from_slash(type_root), &files))
		.collect();
	let remap = |pkg_dir: &str, runtime_path: &str| {
		files
			.root_path(&gopath::join(&[pkg_dir, "default.project.json"]))
			.map_or_else(
				|| runtime_path.to_owned(),
				|root_path| remap_runtime_path(pkg_dir, runtime_path, &root_path),
			)
	};
	let mappings = build_node_modules_path_mappings(
		&type_roots,
		rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		options.custom_conditions.as_deref().unwrap_or_default(),
		&remap,
	);
	Ok(Some(PackageLayout {
		resolvers,
		mapping: mappings.runtime,
		portable_mapping: mappings.portable,
		files,
		manifests: mappings.manifests,
	}))
}

impl PackageLayout {
	/// `fingerprint`: a hash of the resolvers' mappings and the portable path mapping, paths
	/// relative to `dir`.
	pub(crate) fn fingerprint(&self, dir: &str) -> String {
		let separator = gopath::NATIVE.separator();
		let relative = |path: &str| -> String {
			gopath::rel(&gopath::from_slash(dir), &gopath::from_slash(path)).map_or_else(
				|_| gopath::to_slash(path).into_owned(),
				|mut rel| {
					// A wildcard's prefix can end in a separator; keep it so out/* and out*
					// cannot share a fingerprint.
					if path.ends_with('/') || path.ends_with(separator) {
						rel.push(separator);
					}
					gopath::to_slash(&rel).into_owned()
				},
			)
		};
		let resolvers = gojson::array(self.resolvers.iter().map(|resolver| {
			let state = portable_incremental_rojo_state(resolver.state(), &relative);
			gojson::Object::new()
				.raw(
					"partitions",
					&crate::incremental_salt::partitions_json(&state.partitions),
				)
				.raw(
					"filePaths",
					&crate::incremental_salt::file_paths_json(&state.file_paths),
				)
				.finish()
		}));
		let exact = gojson::string_map(
			self.portable_mapping
				.exact
				.iter()
				.map(|(key, value)| (relative(key), relative(value)))
				.collect::<Vec<_>>()
				.iter()
				.map(|(key, value)| (key, value)),
		);
		let patterns = gojson::array(self.portable_mapping.patterns.iter().map(|pattern| {
			gojson::Object::new()
				.string("TypesPrefix", &relative(&pattern.types_prefix))
				.string("TypesSuffix", &gopath::to_slash(&pattern.types_suffix))
				.string("Runtime", &relative(&pattern.runtime))
				.finish()
		}));
		let patterns = if self.portable_mapping.patterns.is_empty() {
			"null".to_owned()
		} else {
			patterns
		};
		let payload = gojson::Object::new()
			.string("policy", "package-default-projects-v1")
			.raw("resolvers", &resolvers)
			.raw(
				"mapping",
				&gojson::Object::new()
					.raw("Exact", &exact)
					.raw("Patterns", &patterns)
					.finish(),
			)
			.finish();
		hex_sha256(payload.as_bytes())
	}
}

/// The lowercase hex SHA-256 of `bytes`.
pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
	let digest = Sha256::digest(bytes);
	let mut out = String::with_capacity(64);
	for byte in digest {
		let _ = write!(out, "{byte:02x}");
	}
	out
}
