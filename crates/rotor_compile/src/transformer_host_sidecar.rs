//! `transformer_host_sidecar.go`: the two transformer hosts, the Node sidecar and an API
//! client's callback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::compile::Error;
use crate::project::{Cancellation, lock};
use crate::sidecar::{
	SidecarCallStats, SidecarSession, apply_transformer_response_metrics, run_transformer_sidecar_validation,
	sidecar_response_timeout,
};
use crate::sidecar_shards::run_sharded_transformer_sidecar;
use crate::timings::{Stage, log_stage, log_stage_named};
use crate::transformer_host::{
	TransformerCall, TransformerCallback, TransformerHost, TransformerResponse, canonical_transformer_request_paths,
	prepare_transformer_request, prepare_transformer_validation_request,
};

/// `sidecarTransformerHost`: the Node subprocess.
pub(crate) struct SidecarTransformerHost;

impl TransformerHost for SidecarTransformerHost {
	fn transform(
		&self,
		cancel: Option<&Cancellation>,
		call: TransformerCall,
	) -> Result<(TransformerResponse, SidecarCallStats), Error> {
		run_sharded_transformer_sidecar(cancel, &call).map_err(|failure| failure.error)
	}

	fn validate(
		&self,
		cancel: Option<&Cancellation>,
		dir: &str,
		config_path: &str,
	) -> Result<(TransformerResponse, SidecarCallStats), Error> {
		run_transformer_sidecar_validation(cancel, dir, config_path)
	}

	fn uses_subprocess_sidecar(&self) -> bool {
		true
	}
}

/// `callbackTransformerHost`: an API client runs each request; a per-project tracker keeps the
/// stamps a worker would.
pub struct CallbackTransformerHost {
	callback: TransformerCallback,
	slots: Mutex<HashMap<String, Arc<Mutex<Option<SidecarSession>>>>>,
}

impl CallbackTransformerHost {
	/// `newCallbackTransformerHost`
	pub(crate) fn new(callback: TransformerCallback) -> Self {
		Self {
			callback,
			slots: Mutex::new(HashMap::new()),
		}
	}

	fn slot(&self, key: &str) -> Arc<Mutex<Option<SidecarSession>>> {
		Arc::clone(lock(&self.slots).entry(key.to_owned()).or_default())
	}
}

impl TransformerHost for CallbackTransformerHost {
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the project's tracker stays held for the whole request, as Go's slot is"
	)]
	fn transform(
		&self,
		cancel: Option<&Cancellation>,
		call: TransformerCall,
	) -> Result<(TransformerResponse, SidecarCallStats), Error> {
		let mut stats = SidecarCallStats::default();
		let timeout = sidecar_response_timeout()?;
		let (dir, config_path, key) = canonical_transformer_request_paths(&call.dir, &call.config_path);
		let slot = self.slot(&key);
		let stop_wait = log_stage(&call.config_path, Stage::SidecarSessionWait);
		let mut tracker = lock(&slot);
		stats.wait = stop_wait();

		let stop_prep = log_stage(&call.config_path, Stage::SidecarPreparation);
		let tracker = tracker.get_or_insert_with(SidecarSession::default);
		let request = prepare_transformer_request(&call, tracker, &dir, &config_path, &mut stats);
		stats.prep += stop_prep();
		let request = request?;

		// `context.WithTimeout`: the callback sees the deadline as Go's does.
		let deadline = Cancellation::with_timeout(cancel, timeout);
		let stage = crate::sidecar::transformer_round_trip_stage_name(&call.config_path, call.plugins.as_deref());
		let stop_round_trip = log_stage_named(&call.config_path, &stage);
		let response = (self.callback)(Some(&deadline), request);
		stats.round_trip = stop_round_trip();
		let response = response.map_err(|error| Error::message(format!("transformer callback failed: {error}")))?;
		if let Some(state) = &call.state {
			lock(state).disk_scanned = true;
		}
		if let Some(transport) = &response.transport {
			stats.request_bytes = transport.request_bytes;
			stats.response_bytes = transport.response_bytes;
		}
		apply_transformer_response_metrics(&mut stats, &response, &call.config_path);
		Ok((response, stats))
	}

	fn validate(
		&self,
		cancel: Option<&Cancellation>,
		dir: &str,
		config_path: &str,
	) -> Result<(TransformerResponse, SidecarCallStats), Error> {
		let mut stats = SidecarCallStats::default();
		let timeout = sidecar_response_timeout()?;
		let (request, _, config_path, key) = prepare_transformer_validation_request(dir, config_path);
		let slot = self.slot(&key);
		let stop_wait = log_stage(&config_path, Stage::SidecarSessionWait);
		let _held = lock(&slot);
		stats.wait = stop_wait();
		let deadline = Cancellation::with_timeout(cancel, timeout);
		let stop_round_trip = log_stage(&config_path, Stage::SidecarRoundTrip);
		let response = (self.callback)(Some(&deadline), request);
		stats.round_trip = stop_round_trip();
		let response = response.map_err(|error| Error::message(format!("transformer callback failed: {error}")))?;
		if let Some(transport) = &response.transport {
			stats.request_bytes = transport.request_bytes;
			stats.response_bytes = transport.response_bytes;
		}
		apply_transformer_response_metrics(&mut stats, &response, &config_path);
		Ok((response, stats))
	}
}
