//! `afterdeclarations.go`: `afterDeclarations` transformers cannot run, since declarations are
//! emitted natively; each config warns once per process.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use rotor_support::logservice;

use crate::incremental_manifest::normalize_source_file_path;
use crate::project::lock;

fn warned() -> &'static Mutex<HashSet<String>> {
	static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
	WARNED.get_or_init(Mutex::default)
}

fn scanned() -> &'static Mutex<HashSet<String>> {
	static SCANNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
	SCANNED.get_or_init(Mutex::default)
}

/// `warnUnsupportedAfterDeclarations`: warns once per config when it configures any.
pub fn warn_unsupported_after_declarations(config_path: &str, count: usize) {
	if count == 0 {
		return;
	}
	if !lock(warned()).insert(normalize_source_file_path(config_path)) {
		return;
	}
	logservice::warn(&format!(
		"{config_path}: afterDeclarations transformers are not supported; declarations are emitted natively"
	));
}

/// `takeAfterDeclarationsScan`: true the first time a config that has not warned is scanned.
#[must_use]
pub fn take_after_declarations_scan(config_path: &str) -> bool {
	let key = normalize_source_file_path(config_path);
	if lock(warned()).contains(&key) {
		return false;
	}
	lock(scanned()).insert(key)
}

/// `countConfiguredAfterDeclarations`: the plugin entries with `"afterDeclarations": true`.
#[must_use]
pub fn count_configured_after_declarations(plugins: &[String]) -> usize {
	plugins
		.iter()
		.filter(|plugin| {
			crate::gojson::parse(plugin.as_bytes())
				.ok()
				.and_then(|entry| entry.field("afterDeclarations").cloned())
				== Some(crate::gojson::Value::Bool(true))
		})
		.count()
}
