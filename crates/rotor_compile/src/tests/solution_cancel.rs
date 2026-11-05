//! `solution_cancel_test.go`.

use rotor_tsrs::Context;

use super::{large_stack, write_project};
use crate::project::{Cancellation, ProjectOptions, new_project_program_with_options};
use crate::solution::{SolutionCoordinator, SolutionProjectStatus, SolutionRoot};

#[test]
fn cancelled_multi_config_build_ends_with_cancellation() {
	// Two real configs built together under an already-cancelled context.
	let cancellation = Cancellation::default();
	cancellation.cancel();
	let temps: Vec<_> = ["@scope/cancel-production", "@scope/cancel-tests"]
		.into_iter()
		.map(|name| {
			let temp = write_project(name, "");
			for file in ["first.ts", "second.ts"] {
				temp.write(&format!("src/{file}"), "export const value = 1;\n");
			}
			temp
		})
		.collect();
	let roots: Vec<SolutionRoot> = temps
		.iter()
		.map(|temp| SolutionRoot {
			config_path: temp.join("tsconfig.json").to_string_lossy().into_owned(),
			options: ProjectOptions {
				context: Some(cancellation.clone()),
				..ProjectOptions::default()
			},
		})
		.collect();
	large_stack(|| {
		let mut coordinator = SolutionCoordinator::for_root_options(&roots).expect("the coordinator");
		let output = coordinator.drain_with_project_results_for_selection(None);
		let error = output.error.map(|error| error.to_string());
		assert!(
			error.as_deref().is_some_and(|error| error.contains("context canceled")),
			"drain error = {error:?}, want context.Canceled"
		);
		for project in &output.project_results {
			assert_ne!(
				project.status,
				SolutionProjectStatus::Success,
				"project {} succeeded under a cancelled context",
				project.config_path
			);
		}
	});
}

#[test]
fn global_diagnostics_under_cancelled_context_leaves_checkers_usable() {
	let temp = write_project("@scope/cancel-gate-three", "");
	for file in ["first.ts", "second.ts"] {
		temp.write(&format!("src/{file}"), "export const value = 1;\n");
	}
	large_stack(|| {
		let (_, program) = new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {failure} ({:?})", failure.messages()));
		let context = Context::default();
		for &file in program.source_files() {
			crate::compile::pre_emit_project_file_diagnostics_with_options(
				&context,
				program,
				file,
				&ProjectOptions::default(),
			);
		}
		// Gate 3 runs on its own context, which the build's cancellation never reaches.
		program.get_global_diagnostics(&context);
		program.for_each_checker_parallel(|_, checker| {
			assert!(
				!checker.was_canceled(),
				"gate 3 under a cancelled context marked a checker cancelled"
			);
		});
	});
}

#[test]
fn multi_config_build_cancelled_at_any_point_ends_cleanly() {
	// Given: the number of cancellation checks a full two-config build makes.
	let total = drain_two_configs(i64::MAX);
	assert!(total > 0, "the build never checked its context");
	// When: the cancellation turns on after each possible number of checks, which lands the
	// cancel inside the precheck loop, between precheck and gate 3, and in every later stage.
	for after in 0..=total {
		// Then: drain_two_configs failed the test on a panic or on an error that is not a
		// cancellation.
		drain_two_configs(after);
	}
}

/// `drainTwoConfigs`: builds two fresh configs under a countdown cancellation and returns how
/// many times the build checked it. Only success or a cancellation may result.
fn drain_two_configs(after: i64) -> i64 {
	let (cancellation, countdown) = Cancellation::countdown(after);
	let temps: Vec<_> = ["@scope/countdown-production", "@scope/countdown-tests"]
		.into_iter()
		.map(|name| {
			let temp = write_project(name, "");
			for file in ["first.ts", "second.ts", "third.ts"] {
				temp.write(
					&format!("src/{file}"),
					"export function value(n: number) { return n + 1; }\n",
				);
			}
			temp
		})
		.collect();
	let roots: Vec<SolutionRoot> = temps
		.iter()
		.map(|temp| SolutionRoot {
			config_path: temp.join("tsconfig.json").to_string_lossy().into_owned(),
			options: ProjectOptions {
				context: Some(cancellation.clone()),
				..ProjectOptions::default()
			},
		})
		.collect();
	let output = large_stack(|| {
		let mut coordinator = SolutionCoordinator::for_root_options(&roots).unwrap_or_else(|error| panic!("{error}"));
		coordinator.drain_with_project_results_for_selection(None)
	});
	if let Some(error) = output.error {
		let text = error.to_string();
		assert!(
			text.contains("context canceled"),
			"cancel after {after} checks: drain error = {text}, want nil or context.Canceled"
		);
	}
	countdown.calls.load(std::sync::atomic::Ordering::Relaxed)
}
