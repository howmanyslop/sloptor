//! `sidecar_shards.go`: one transform fanned out across several workers when it is large enough
//! to pay for them.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;

use crate::compile::Error;
use crate::project::{Cancellation, lock};
use crate::sidecar::{
	SidecarCallStats, SidecarResult, run_transformer_sidecar_call, run_transformer_sidecar_shard, sidecar_failure,
	sidecar_response_timeout, sidecar_slot_for, spawn_sidecar_session,
};
use crate::transformer_host::{
	TransformerCall, TransformerRequest, TransformerResponse, canonical_transformer_request_paths,
};

/// `sidecarShardsEnv`
const SIDECAR_SHARDS_ENV: &str = "ROTOR_SIDECAR_SHARDS";
/// `sidecarShardMaxDefault`: past four, the import closure each worker parses outweighs the
/// transform time it splits.
const SIDECAR_SHARD_MAX_DEFAULT: usize = 4;
/// `sidecarShardMinFiles`: below this many files per shard another worker costs more than it
/// takes on.
pub(crate) const SIDECAR_SHARD_MIN_FILES: usize = 256;

/// `buildShare`: how many builds share this machine; the CLI updates it while admitted.
static BUILD_SHARE: AtomicI32 = AtomicI32::new(0);

/// `SetBuildShare`
pub fn set_build_share(share: i32) {
	BUILD_SHARE.store(share.max(1), Ordering::Relaxed);
}

/// `sidecarShardCount`: how many workers a transform of `file_count` files fans out across.
/// `ROTOR_SIDECAR_SHARDS` overrides the CPU-derived default, which concurrent builds split.
pub(crate) fn sidecar_shard_count(file_count: usize) -> usize {
	sidecar_shard_count_with(file_count, crate::sidecar::getenv(SIDECAR_SHARDS_ENV).as_deref())
}

/// [`sidecar_shard_count`] with `ROTOR_SIDECAR_SHARDS` read as `configured`.
pub(crate) fn sidecar_shard_count_with(file_count: usize, configured: Option<&str>) -> usize {
	let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
	let share = usize::try_from(BUILD_SHARE.load(Ordering::Relaxed).max(1)).unwrap_or(1);
	let configured = configured
		.and_then(|raw| raw.parse::<i64>().ok())
		.filter(|&configured| configured >= 1);
	let limit = configured.map_or_else(
		|| (SIDECAR_SHARD_MAX_DEFAULT.min((cpus / 2).max(1)) / share).max(1),
		|configured| usize::try_from(configured).unwrap_or(usize::MAX),
	);
	limit.min(file_count / SIDECAR_SHARD_MIN_FILES).max(1)
}

/// `splitSidecarShards`: `count` groups of similar total text length, largest file first onto
/// the lightest shard. Each shard keeps the caller's file order.
pub(crate) fn split_sidecar_shards(files: &[P<SourceFile>], count: usize) -> Vec<Vec<P<SourceFile>>> {
	if files.is_empty() {
		return Vec::new();
	}
	let mut order: Vec<usize> = (0..files.len()).collect();
	order.sort_by(|&left, &right| {
		files[right]
			.text()
			.len()
			.cmp(&files[left].text().len())
			.then_with(|| files[left].file_name().cmp(files[right].file_name()))
	});
	let mut loads = vec![0usize; count];
	let mut owner = vec![0usize; files.len()];
	for index in order {
		let mut lightest = 0;
		for shard in 1..count {
			if loads[shard] < loads[lightest] {
				lightest = shard;
			}
		}
		owner[index] = lightest;
		loads[lightest] += files[index].text().len() + 1;
	}
	let mut shards = vec![Vec::new(); count];
	for (index, file) in files.iter().enumerate() {
		shards[owner[index]].push(*file);
	}
	shards.retain(|shard| !shard.is_empty());
	shards
}

/// `runShardedTransformerSidecar`: the configured transformers are per-file, so each shard sees
/// the same checker answers about its files that a single worker would.
pub(crate) fn run_sharded_transformer_sidecar(cancel: Option<&Cancellation>, call: &TransformerCall) -> SidecarResult {
	let count = sidecar_shard_count(call.compile_files.len());
	// A configured parallel split already divides the request between its own two workers.
	if count <= 1 || call.parallel.is_some() {
		return run_transformer_sidecar_call(cancel, call);
	}
	let shards = split_sidecar_shards(&call.compile_files, count);
	let shard_calls: Vec<TransformerCall> = shards
		.into_iter()
		.map(|files| {
			// Each shard reads the build state but must not publish to it while the others read;
			// the merged result is published below.
			let state = call
				.state
				.as_ref()
				.map(|state| Arc::new(Mutex::new(lock(state).clone())));
			TransformerCall {
				compile_files: files,
				state,
				..call.clone()
			}
		})
		.collect();
	let results: Vec<SidecarResult> = std::thread::scope(|scope| {
		#[expect(clippy::needless_collect, reason = "every shard starts before the first join")]
		let handles: Vec<_> = shard_calls
			.iter()
			.enumerate()
			.map(|(index, shard_call)| scope.spawn(move || run_transformer_sidecar_shard(cancel, index, shard_call)))
			.collect();
		handles
			.into_iter()
			.map(|handle| {
				handle.join().unwrap_or_else(|_| {
					Err(sidecar_failure(
						Error::message("compile: transformer sidecar shard panicked"),
						SidecarCallStats::default(),
					))
				})
			})
			.collect()
	});
	let stats: Vec<SidecarCallStats> = results
		.iter()
		.map(|result| match result {
			Ok((_, stats)) => stats.clone(),
			Err(failure) => failure.stats.clone(),
		})
		.collect();
	let merged = merge_sidecar_shard_stats(&stats);
	let mut responses = Vec::with_capacity(results.len());
	for result in results {
		match result {
			Ok((response, _)) => responses.push(response),
			Err(failure) => return Err(sidecar_failure(failure.error, merged)),
		}
	}
	if let Some(state) = &call.state {
		lock(state).disk_scanned = true;
	}
	let mut response = TransformerResponse::default();
	for part in responses {
		response.diagnostics.extend(part.diagnostics);
		response.transformed.extend(part.transformed);
		response.after_declarations_transformers = response
			.after_declarations_transformers
			.max(part.after_declarations_transformers);
		if part.metrics.is_some() && response.metrics.is_none() {
			response.metrics = part.metrics;
		}
	}
	Ok((response, merged))
}

/// `mergeSidecarShardStats`: durations are the fan-out's wall time, so they take the slowest
/// shard; counts and CPU add.
pub(crate) fn merge_sidecar_shard_stats(parts: &[SidecarCallStats]) -> SidecarCallStats {
	let mut merged = SidecarCallStats::default();
	for part in parts {
		merged.wait = merged.wait.max(part.wait);
		merged.prep = merged.prep.max(part.prep);
		merged.round_trip = merged.round_trip.max(part.round_trip);
		merged.decode += part.decode;
		merged.request_bytes += part.request_bytes;
		merged.response_bytes += part.response_bytes;
		merged.stats += part.stats;
		merged.reads += part.reads;
		merged.changed_files += part.changed_files;
		merged.spawned |= part.spawned;
		merged.restarted |= part.restarted;
		merged.node_wall_ms = merged.node_wall_ms.max(part.node_wall_ms);
		merged.node_cpu_user_us += part.node_cpu_user_us;
		merged.node_cpu_system_us += part.node_cpu_system_us;
		if !part.node_version.is_empty() {
			merged.node_version.clone_from(&part.node_version);
		}
	}
	merged
}

/// `warmTransformerSidecarShards`: starts, in the background, the workers a transform of
/// `file_count` files fans out across, each loading TypeScript, the config and the plugins. Each
/// shard's slot is taken before this returns, so an early transform waits for its warm worker.
/// Failures are dropped: the transform spawns its own worker.
pub(crate) fn warm_transformer_sidecar_shards(
	dir: &str,
	config_path: &str,
	plugins: Option<&[String]>,
	file_count: usize,
) {
	let Ok(sidecar_dir) = crate::sidecar_install::resolve_sidecar_dir() else {
		return;
	};
	let Ok(timeout) = sidecar_response_timeout() else {
		return;
	};
	let (dir_path, sidecar_config_path, key) = canonical_transformer_request_paths(dir, config_path);
	let payload = TransformerRequest {
		protocol: 1,
		operation: "warm".to_owned(),
		ts_config_path: gopath::from_slash(&sidecar_config_path).into_owned(),
		project_dir: gopath::from_slash(&dir_path).into_owned(),
		plugins: plugins.map(<[String]>::to_vec).unwrap_or_default(),
		..TransformerRequest::default()
	}
	.to_json();
	for shard in 0..sidecar_shard_count(file_count) {
		let slot_key = if shard > 0 {
			format!("{key}#{shard}")
		} else {
			key.clone()
		};
		let mut guard = sidecar_slot_for(&slot_key).acquire();
		let dir_path = dir_path.clone();
		let sidecar_dir = sidecar_dir.clone();
		let payload = payload.clone();
		let _ = std::thread::Builder::new()
			.name("rotor-sidecar-warm".to_owned())
			.spawn(move || {
				if guard.session.as_ref().is_some_and(|session| !session.dead) {
					return;
				}
				let Ok(session) = spawn_sidecar_session(&dir_path, &sidecar_dir) else {
					return;
				};
				let session = guard.session.insert(session);
				if session
					.write_and_read(None, Instant::now() + timeout, payload.as_bytes())
					.is_err()
				{
					session.close();
					drop(guard.session.take());
					return;
				}
				session.drain_stderr();
			});
	}
}
