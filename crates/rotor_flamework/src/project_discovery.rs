//! `project_discovery.go`: a project's package root, paths, Rojo resolver and build info.

use rotor_rojo::RojoResolver;
use rotor_support::gopath;
use rotor_support::gostr::quote;

use crate::build_info::BuildInfo;
use crate::json_syntax::{self, Value};
use crate::project::upstream_boundary_error;
use crate::state::FLAMEWORK_VERSION;
use crate::{Error, ErrorKind};

pub(crate) struct ProjectRoot {
	pub(crate) directory: String,
	pub(crate) package_directory: String,
	pub(crate) package_name: String,
	pub(crate) package_version: String,
}

/// The `name` and `version` a Go `struct { Name string; Version string }` decodes from a
/// `package.json`, or `None` when `json.Unmarshal` fails.
pub(crate) fn package_identity(data: &[u8]) -> Option<(String, String)> {
	let document = json_syntax::parse(data).ok()?;
	if matches!(document, Value::Null) {
		return Some((String::new(), String::new()));
	}
	if !matches!(document, Value::Object(_)) {
		return None;
	}
	let field = |key: &str| -> Option<String> {
		match document.field(key) {
			None | Some(Value::Null) => Some(String::new()),
			Some(Value::String(text)) => Some(text.clone()),
			Some(_) => None,
		}
	};
	Some((field("name")?, field("version")?))
}

/// The `name` a Go `struct { Name string }` decodes from a `package.json`, or `None` when
/// `json.Unmarshal` fails. Only the name: a `version` of any type decodes.
pub(crate) fn package_name(data: &[u8]) -> Option<String> {
	match json_syntax::parse(data).ok()? {
		Value::Null => Some(String::new()),
		document @ Value::Object(_) => match document.field("name") {
			None | Some(Value::Null) => Some(String::new()),
			Some(Value::String(text)) => Some(text.clone()),
			Some(_) => None,
		},
		_ => None,
	}
}

pub(crate) fn open_project_root(directory: &str) -> Result<ProjectRoot, Error> {
	let project_directory =
		crate::abs(directory).map_err(|error| Error::msg(format!("resolve Flamework project directory: {error}")))?;
	let package_path = find_package_json(&project_directory);
	if package_path.is_empty() {
		return Err(upstream_boundary_error(format!(
			"package.json not found in {project_directory}"
		)));
	}
	let package_data = std::fs::read(&package_path)
		.map_err(|error| Error::io("open", &package_path, &error).wrap("read Flamework package identity"))?;
	let Some((name, version)) = package_identity(&package_data).filter(|(name, _)| !name.is_empty()) else {
		return Err(Error::kinded(ErrorKind::InvalidPackage, &format!(" at {package_path}")));
	};
	Ok(ProjectRoot {
		directory: project_directory,
		package_directory: gopath::dir(&package_path),
		package_name: name,
		package_version: version,
	})
}

pub(crate) fn resolve_project_path(project_directory: &str, configured: &str, fallback: &str) -> Result<String, Error> {
	let value = if configured.is_empty() {
		gopath::join(&[project_directory, fallback])
	} else if gopath::is_abs(configured) {
		configured.to_owned()
	} else {
		gopath::join(&[project_directory, configured])
	};
	let resolved =
		crate::abs(&value).map_err(|error| Error::msg(format!("resolve Flamework path {}: {error}", quote(&value))))?;
	Ok(gopath::clean(&resolved))
}

pub(crate) fn open_rojo_resolver(
	project_directory: &str,
	out_directory: &str,
	configured: &str,
) -> Result<RojoResolver, Error> {
	let config_path = if configured.is_empty() {
		rotor_rojo::find_rojo_config_file_path(project_directory)
			.0
			.unwrap_or_default()
	} else if gopath::is_abs(configured) {
		configured.to_owned()
	} else {
		gopath::join(&[project_directory, configured])
	};
	if config_path.is_empty() {
		return Ok(RojoResolver::synthetic(out_directory));
	}
	if let Err(error) = std::fs::metadata(&config_path) {
		return Err(Error::io("stat", &config_path, &error).wrap("read Flamework Rojo configuration"));
	}
	Ok(RojoResolver::from_path(&config_path))
}

/// `findPackageJSON`: the nearest regular `package.json` at or above `directory`, or `""`.
pub(crate) fn find_package_json(directory: &str) -> String {
	let mut current = gopath::clean(directory);
	loop {
		let candidate = gopath::join(&[&current, "package.json"]);
		if std::fs::metadata(&candidate).is_ok_and(|info| info.is_file()) {
			return candidate;
		}
		let parent = gopath::dir(&current);
		if parent == current {
			return String::new();
		}
		current = parent;
	}
}

pub(crate) fn load_project_build_info(project_directory: &str, package_directory: &str) -> Result<BuildInfo, Error> {
	let mut path = gopath::join(&[project_directory, "flamework.build"]);
	if std::fs::metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
		&& project_directory != package_directory
	{
		let package_path = gopath::join(&[package_directory, "flamework.build"]);
		if std::fs::metadata(&package_path).is_ok() {
			path = package_path;
		}
	}
	BuildInfo::load(&path, FLAMEWORK_VERSION).map_err(|error| error.wrap("load project Flamework build info"))
}
