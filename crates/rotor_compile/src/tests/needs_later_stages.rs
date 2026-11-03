//! Go tests in `internal/compile` whose subject is Stage 5b (the transformer-plugin sidecar,
//! the solution builder) or Stage 6 (native Flamework). Each is ported as an ignored test with
//! its Go name, so the stage that adds the code turns it on.

macro_rules! later {
	($($name:ident => $reason:literal;)*) => {
		$(
			#[test]
			#[ignore = $reason]
			fn $name() {}
		)*
	};
}

later! {
	task7_runtime_fixture_matches_pinned_flamework_upstream_oracle => "needs Stage 6: native Flamework and the pinned upstream Flamework oracle (task7_runtime_test.go)";
	task7_diag002_dirty_incremental_state_matches_public_upstream => "needs Stage 6: native Flamework incremental state (task7_diag_dirty_state_test.go)";
	task7_diag002_malformed_persisted_state_fails_closed => "needs Stage 6: native Flamework incremental state (task7_diag_dirty_state_test.go)";
	task7_flamework_clean_warm_incremental_and_recovery_are_deterministic => "needs Stage 6: native Flamework (task7_acceptance_test.go)";
	task7_flamework_public_options_and_defaults_reach_real_compiler_pipeline => "needs Stage 6: native Flamework (task7_acceptance_test.go)";
	task7_flamework_invalid_inputs_fail_closed_and_preserve_last_good_artifacts => "needs Stage 6: native Flamework (task7_acceptance_test.go)";
	task7_flamework_invalid_project_state_fails_closed_and_preserves_artifacts => "needs Stage 6: native Flamework (task7_acceptance_test.go)";
	source_map_trace_retains_generated_only_lines => "needs 5b: a transformer plugin through the sidecar (sourcemap_plugin_acceptance_test.go)";
	source_map_trace_deduplicates_composed_positions => "needs 5b: a transformer plugin through the sidecar (sourcemap_plugin_acceptance_test.go)";
	source_map_flamework_nested_callbacks => "needs Stage 6: native Flamework (sourcemap_plugin_acceptance_test.go)";
	source_map_performance_goldens => "needs 5b: the sidecar's performance fixture (sourcemap_performance_acceptance_test.go)";
	react_syntax_origin_survives_both_transports => "needs 5b: the sidecar transformer host (react_syntax_origin_test.go)";
	react_syntax_origin_initial_prefix_suffix_and_next_build => "needs 5b and Stage 6: plugins around native Flamework (react_syntax_origin_test.go)";
}
