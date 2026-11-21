//! `diagservice.go`: the port of `classes/DiagnosticService.ts`. Upstream is a global static
//! accumulator; rotor injects an instance so concurrent compilations stay independent.

use std::hash::{BuildHasher, Hash};

use rustc_hash::FxHashSet;

use crate::diagnostics::Diagnostic;

/// The transformer's diagnostic accumulator (`DiagService`).
#[derive(Debug, Default)]
pub struct DiagService {
	diagnostics: Vec<Diagnostic>,
	/// The codes already reported through [`DiagService::add_single`] (upstream
	/// `singleDiagnostics`, keyed by the stable factory name).
	single: FxHashSet<&'static str>,
}

impl DiagService {
	/// An empty accumulator (`NewDiagService`).
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Appends a diagnostic unconditionally (upstream `addDiagnostic`).
	pub fn add(&mut self, diagnostic: Diagnostic) {
		self.diagnostics.push(diagnostic);
	}

	/// Appends `diagnostic` unless one with the same code was already added through this method
	/// since the last flush (upstream `addSingleDiagnostic`).
	pub fn add_single(&mut self, diagnostic: Diagnostic) {
		if self.single.insert(diagnostic.code) {
			self.add(diagnostic);
		}
	}

	/// Appends `diagnostic` unless `key` is already in the caller's `cache`, recording it on
	/// first use (upstream `addDiagnosticWithCache`).
	pub fn add_with_cache<K: Eq + Hash, S: BuildHasher>(
		&mut self,
		key: K,
		diagnostic: Diagnostic,
		cache: &mut std::collections::HashSet<K, S>,
	) {
		if cache.insert(key) {
			self.add(diagnostic);
		}
	}

	/// Returns the accumulated diagnostics and resets the service, the single-report set
	/// included (upstream `flush`).
	pub fn flush(&mut self) -> Vec<Diagnostic> {
		self.single.clear();
		std::mem::take(&mut self.diagnostics)
	}

	/// Whether any pending diagnostic is an error (upstream `hasErrors`).
	#[must_use]
	pub fn has_errors(&self) -> bool {
		self.diagnostics.iter().any(|diagnostic| !diagnostic.warning)
	}

	/// The pending diagnostics, in report order.
	#[must_use]
	pub fn pending(&self) -> &[Diagnostic] {
		&self.diagnostics
	}
}
