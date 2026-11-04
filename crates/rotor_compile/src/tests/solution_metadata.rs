//! `solution_metadata_test.go`.

use std::collections::HashMap;
use std::sync::mpsc::{channel, sync_channel};
use std::sync::{Arc, Mutex};

use rotor_support::{fsutil, gopath};

use super::solution::{RecordingDrainer, coordinator_with, join, write_solution_config};
use super::{NO_LIB_GLOBAL_STUBS, Temp};
use crate::crossproject::{SolutionWriteMetadata, populate_cross_project_metadata};
use crate::project::ProjectOptions;
use crate::solution::build_solution_graph;
use crate::solution_scheduler::{SolutionTask, run_solution_tasks};

/// `writeMetadataSolutionProject`
pub fn write_metadata_solution_project(dir: &str, out_dir: &str) {
	std::fs::create_dir_all(join(dir, &["src"])).expect("the directory");
	let config = format!(
		r#"{{"compilerOptions":{{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":{}}},"include":["src"]}}"#,
		rotor_support::gostr::quote(out_dir)
	);
	for (path, content) in [
		("tsconfig.json", config.as_str()),
		("package.json", r#"{"name":"solution-metadata-project"}"#),
		("src/globals.d.ts", NO_LIB_GLOBAL_STUBS),
		("src/main.ts", "export const value = 1;\n"),
	] {
		std::fs::write(join(dir, &[path]), content).expect("the file writes");
	}
}

/// `solutionWriteMetadataForTest`
fn solution_write_metadata_for_test(config_path: &str, entry: &ProjectOptions) -> SolutionWriteMetadata {
	let mut graph =
		build_solution_graph(config_path, entry).unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	populate_cross_project_metadata(&mut graph)
}

/// `canonicalMetadataPath`
fn canonical_metadata_path(path: &str) -> String {
	let abs = gopath::abs(path).expect("abs");
	fsutil::eval_symlinks(&abs).map_or_else(|_| gopath::clean(&abs), |resolved| gopath::clean(&resolved))
}

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test's table")]
fn solution_write_roots() {
	struct Case {
		name: &'static str,
		emit_include: bool,
		include_path: Option<&'static [&'static str]>,
		first_out: &'static [&'static str],
		second_out: &'static [&'static str],
		want_edge: bool,
	}
	let cases = [
		Case {
			name: "disjoint outputs",
			emit_include: false,
			include_path: None,
			first_out: &["outputs", "first"],
			second_out: &["outputs", "second"],
			want_edge: false,
		},
		Case {
			name: "identical outputs",
			emit_include: false,
			include_path: None,
			first_out: &["outputs"],
			second_out: &["outputs"],
			want_edge: true,
		},
		Case {
			name: "nested outputs",
			emit_include: false,
			include_path: None,
			first_out: &["outputs"],
			second_out: &["outputs", "nested"],
			want_edge: true,
		},
		Case {
			name: "shared include output enabled",
			emit_include: true,
			include_path: Some(&["shared-include"]),
			first_out: &["outputs", "first"],
			second_out: &["outputs", "second"],
			want_edge: true,
		},
		Case {
			name: "shared include output disabled",
			emit_include: false,
			include_path: Some(&["shared-include"]),
			first_out: &["outputs", "first"],
			second_out: &["outputs", "second"],
			want_edge: false,
		},
	];
	for case in cases {
		let temp = Temp::new();
		let root = temp.dir();
		let first_dir = join(&root, &["first"]);
		let second_dir = join(&root, &["second"]);
		write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
		let first_out = join(&root, case.first_out);
		let second_out = join(&root, case.second_out);
		let include_path = case.include_path.map(|parts| join(&root, parts));
		for directory in [Some(&first_out), Some(&second_out), include_path.as_ref()]
			.into_iter()
			.flatten()
		{
			std::fs::create_dir_all(directory).expect("the directory");
		}
		write_metadata_solution_project(&first_dir, &first_out);
		write_metadata_solution_project(&second_dir, &second_out);
		let entry = ProjectOptions {
			emit_include_files: case.emit_include,
			include_path: include_path.clone().unwrap_or_default(),
			..ProjectOptions::default()
		};
		let metadata = solution_write_metadata_for_test(&join(&root, &["tsconfig.json"]), &entry);
		let coordinator = coordinator_with(
			&join(&root, &["tsconfig.json"]),
			&entry,
			Arc::new(RecordingDrainer::default()),
		);
		let first_config = join(&first_dir, &["tsconfig.json"]);
		let second_config = join(&second_dir, &["tsconfig.json"]);
		let mut want_first = vec![canonical_metadata_path(&first_out)];
		let mut want_second = vec![canonical_metadata_path(&second_out)];
		if case.emit_include
			&& let Some(include_path) = &include_path
		{
			want_first.push(canonical_metadata_path(include_path));
			want_second.push(canonical_metadata_path(include_path));
		}
		assert_eq!(
			metadata.write_roots[&first_config], want_first,
			"{}: first write roots",
			case.name
		);
		assert_eq!(
			metadata.write_roots[&second_config], want_second,
			"{}: second write roots",
			case.name
		);
		if case.want_edge {
			let want: HashMap<String, Vec<String>> = [(second_config, vec![first_config])].into();
			assert_eq!(
				coordinator.wait_only_dependencies, want,
				"{}: wait-only dependencies",
				case.name
			);
		} else {
			assert!(
				coordinator.wait_only_dependencies.is_empty(),
				"{}: wait-only dependencies = {:?}, want none",
				case.name,
				coordinator.wait_only_dependencies
			);
		}
	}
}

#[test]
fn solution_write_roots_leaves_invalid_config_empty() {
	let temp = Temp::new();
	let root = temp.dir();
	let child_dir = join(&root, &["child"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	temp.write("child/tsconfig.json", r#"{"compilerOptions":{"outDir":42}}"#);
	let child_config = join(&child_dir, &["tsconfig.json"]);
	let metadata = solution_write_metadata_for_test(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	let got = metadata.write_roots.get(&child_config).cloned().unwrap_or_default();
	assert!(got.is_empty(), "invalid config write roots = {got:?}, want empty");
}

#[cfg(unix)]
#[test]
fn solution_write_roots_canonicalizes_symlink_alias() {
	let temp = Temp::new();
	let root = temp.dir();
	let shared_out = join(&root, &["shared-out"]);
	let alias_out = join(&root, &["shared-out-alias"]);
	std::fs::create_dir_all(&shared_out).expect("the directory");
	std::os::unix::fs::symlink(&shared_out, &alias_out).expect("symlinks");
	let first_dir = join(&root, &["first"]);
	let second_dir = join(&root, &["second"]);
	write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
	write_metadata_solution_project(&first_dir, &shared_out);
	write_metadata_solution_project(&second_dir, &alias_out);
	let metadata = solution_write_metadata_for_test(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	let first = &metadata.write_roots[&join(&first_dir, &["tsconfig.json"])];
	let second = &metadata.write_roots[&join(&second_dir, &["tsconfig.json"])];
	assert_eq!(first, second, "symlinked write roots differ");
}

#[test]
fn solution_conflict_dependencies() {
	let temp = Temp::new();
	let root = temp.dir();
	let first_dir = join(&root, &["first"]);
	let second_dir = join(&root, &["second"]);
	let root_config = join(&root, &["tsconfig.json"]);
	write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
	write_metadata_solution_project(&first_dir, &join(&root, &["outputs", "same"]));
	write_metadata_solution_project(&second_dir, &join(&root, &["outputs", "same"]));
	let mut coordinator = coordinator_with(
		&root_config,
		&ProjectOptions::default(),
		Arc::new(RecordingDrainer::default()),
	);
	let second_config = join(&second_dir, &["tsconfig.json"]);
	let first_config = join(&first_dir, &["tsconfig.json"]);
	let want: HashMap<String, Vec<String>> = [(second_config, vec![first_config])].into();
	assert_eq!(coordinator.wait_only_dependencies, want, "wait-only dependencies");
	write_metadata_solution_project(&second_dir, &join(&root, &["outputs", "different"]));
	coordinator
		.reload(&root_config, &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("Reload: {error}"));
	assert!(
		coordinator.wait_only_dependencies.is_empty(),
		"reloaded wait-only dependencies = {:?}, want none",
		coordinator.wait_only_dependencies
	);
}

#[test]
fn solution_conflict_dependencies_scheduler() {
	for conflict in [true, false] {
		let tasks = vec![
			SolutionTask::default(),
			SolutionTask {
				index: 1,
				wait_only: if conflict { vec![0] } else { Vec::new() },
				..SolutionTask::default()
			},
		];
		let (started, started_rx) = channel::<usize>();
		let (release, release_rx) = sync_channel::<()>(0);
		let release_rx = Mutex::new(release_rx);
		let (done, done_rx) = channel();
		std::thread::spawn(move || {
			let outcomes = run_solution_tasks(&tasks, 2, |index| -> Result<(), String> {
				let _ = started.send(index);
				let _ = crate::project::lock(&release_rx).recv();
				Ok(())
			});
			let _ = done.send(outcomes);
		});
		if conflict {
			let got = started_rx.recv().expect("a start");
			assert_eq!(got, 0, "started task {got}, want 0");
			std::thread::sleep(std::time::Duration::from_millis(20));
			if let Ok(got) = started_rx.try_recv() {
				panic!("conflicting task {got} started before predecessor finished");
			}
			release.send(()).expect("the release");
			let got = started_rx.recv().expect("a start");
			assert_eq!(got, 1, "started task {got}, want 1");
		} else {
			let mut seen = [false; 2];
			for _ in 0..2 {
				let index = started_rx.recv().expect("a start");
				assert!(!seen[index], "unexpected task index {index}");
				seen[index] = true;
			}
		}
		drop(release);
		for (index, error) in done_rx.recv().expect("the outcomes").iter().enumerate() {
			assert!(error.is_none(), "task {index} error = {error:?}, want nil");
		}
	}
}
