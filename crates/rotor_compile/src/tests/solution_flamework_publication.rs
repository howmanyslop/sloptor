//! `solution_flamework_publication_test.go`

use std::sync::{Arc, Mutex};

use rotor_support::gopath;

use super::Temp;
use super::solution::{builders, coordinator_with, join, write_solution_config};
use super::solution_drain::must_read_file;
use crate::compile::Error;
use crate::output::BuildResult;
use crate::project::{PendingPersist, lock};
use crate::solution::{DrainResult, SolutionProject, SolutionProjectDrainer};

/// `flameworkSolutionPublicationDrainer`
#[derive(Default)]
struct PublicationDrainer {
	artifact_path: String,
	next: Mutex<Vec<u8>>,
	fail_publish: bool,
	drained: Mutex<Vec<String>>,
	observed: Mutex<String>,
}

fn built(name: String, output: Vec<u8>) -> DrainResult {
	DrainResult {
		result: Some(BuildResult {
			outputs: [(name, output)].into(),
			..BuildResult::default()
		}),
		..DrainResult::default()
	}
}

impl SolutionProjectDrainer for PublicationDrainer {
	fn drain(&self, project: SolutionProject) -> DrainResult {
		let name = gopath::base(&gopath::dir(&project.config_path));
		lock(&self.drained).push(name.clone());
		if name == "package" {
			let fail_publish = self.fail_publish;
			let package_root = gopath::dir(&project.config_path);
			let artifact = gopath::base(&self.artifact_path);
			let next = lock(&self.next).clone();
			let persist: PendingPersist = Box::new(move || {
				if fail_publish {
					return Err(Error::message("publish failed"));
				}
				rotor_flamework::persist_artifacts(
					&package_root,
					&[rotor_flamework::Artifact {
						path: artifact.clone(),
						data: Some(next.clone()),
					}],
				)
				.map_err(|error| Error::message(error.to_string()))
			});
			let persists = project
				.options
				.pending_solution_dependency_persists
				.as_ref()
				.or(project.options.pending_solution_persists.as_ref())
				.expect("pending persists");
			lock(persists).push(persist);
			return built("package".to_owned(), b"compiled".to_vec());
		}
		if name == "game" {
			let published = match std::fs::read(&self.artifact_path) {
				Ok(published) => published,
				Err(error) => {
					return DrainResult {
						error: Some(Error::message(error.to_string())),
						..DrainResult::default()
					};
				}
			};
			*lock(&self.observed) = String::from_utf8_lossy(&published).into_owned();
			return built("game".to_owned(), published);
		}
		built(name, b"compiled".to_vec())
	}
}

#[test]
fn solution_flamework_package_publishes_before_dependent_compiles() {
	// Given: a package and dependent game with distinct old and pending build data.
	let (_temp, root, package_config, _) = write_flamework_publication_solution(false);
	let artifact_path = join(&gopath::dir(&package_config), &["flamework.build"]);
	std::fs::write(&artifact_path, r#"{"identifiers":{"service":"old-id"}}"#).expect("write");
	let drainer = Arc::new(PublicationDrainer {
		artifact_path,
		next: Mutex::new(br#"{"identifiers":{"service":"new-id"}}"#.to_vec()),
		..PublicationDrainer::default()
	});
	let mut coordinator = coordinator_with(
		&root,
		&builders(2),
		Arc::clone(&drainer) as Arc<dyn SolutionProjectDrainer>,
	);

	// When: the real solution coordinator drains the package-to-game graph.
	let (_, _, error) = coordinator.drain();
	// Then: the game observes the newly published identifier in the same drain.
	assert!(error.is_none(), "Drain: {error:?}");
	let observed = lock(&drainer.observed).clone();
	assert_eq!(
		observed, r#"{"identifiers":{"service":"new-id"}}"#,
		"dependent build data"
	);
}

#[test]
fn solution_flamework_package_publish_failure_preserves_state_and_blocks_only_dependent() {
	// Given: a prior successful artifact and a package publication that will fail.
	let (_temp, root, package_config, game_config) = write_flamework_publication_solution(true);
	let artifact_path = join(&gopath::dir(&package_config), &["flamework.build"]);
	let prior = r#"{"identifiers":{"service":"stable-id"}}"#;
	std::fs::write(&artifact_path, prior).expect("write");
	let drainer = Arc::new(PublicationDrainer {
		artifact_path: artifact_path.clone(),
		next: Mutex::new(br#"{"identifiers":{"service":"partial-id"}}"#.to_vec()),
		fail_publish: true,
		..PublicationDrainer::default()
	});
	let mut coordinator = coordinator_with(
		&root,
		&builders(1),
		Arc::clone(&drainer) as Arc<dyn SolutionProjectDrainer>,
	);

	// When: package publication fails before the dependent barrier closes.
	let (_, messages, drain_error) = coordinator.drain();

	// Then: prior state remains, the game is blocked, and its sibling still builds.
	assert!(drain_error.is_some(), "Drain unexpectedly succeeded");
	assert!(
		messages.len() == 2
			&& messages[0].contains("publish dependency state")
			&& messages[1].contains("blocked by failed dependency"),
		"messages = {messages:?}, want publication and dependent-block diagnostics"
	);
	assert_eq!(
		String::from_utf8_lossy(&must_read_file(&artifact_path)),
		prior,
		"artifact after failure"
	);
	assert_eq!(*lock(&drainer.drained), ["package", "sibling"], "drained projects");
	let state = coordinator.project_state(&game_config);
	assert!(
		state.as_ref().is_some_and(|state| state.blocked_by == package_config),
		"game state = {:?}",
		state.map(|state| state.blocked_by)
	);
}

#[test]
fn solution_watch_flamework_package_invalidates_dependent_deterministically() {
	// Given: a successfully built package-to-game solution.
	let (_temp, root, package_config, game_config) = write_flamework_publication_solution(false);
	let artifact_path = join(&gopath::dir(&package_config), &["flamework.build"]);
	std::fs::write(&artifact_path, r#"{"id":"old"}"#).expect("write");
	let drainer = Arc::new(PublicationDrainer {
		artifact_path,
		next: Mutex::new(br#"{"id":"first"}"#.to_vec()),
		..PublicationDrainer::default()
	});
	let mut coordinator = coordinator_with(
		&root,
		&builders(2),
		Arc::clone(&drainer) as Arc<dyn SolutionProjectDrainer>,
	);
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "initial Drain: {error:?}");
	lock(&drainer.drained).clear();
	*lock(&drainer.next) = br#"{"id":"second"}"#.to_vec();

	// When: solution watch invalidates the package project.
	let invalidated = coordinator.invalidate(std::slice::from_ref(&package_config));
	let (_, _, drain_error) = coordinator.drain();

	// Then: package and dependent rebuild once in graph order using new state.
	assert!(drain_error.is_none(), "watch Drain: {drain_error:?}");
	assert_eq!(invalidated, [package_config, game_config], "Invalidate()");
	assert_eq!(*lock(&drainer.drained), ["package", "game"], "drained projects");
	assert_eq!(*lock(&drainer.observed), r#"{"id":"second"}"#, "dependent build data");
}

/// `writeFlameworkPublicationSolution`: the temporary root, then the solution, package and game
/// configs.
fn write_flamework_publication_solution(sibling: bool) -> (Temp, String, String, String) {
	let temp = Temp::new();
	let root = temp.dir();
	let mut references = vec!["./package"];
	if sibling {
		references.push("./sibling");
	}
	references.push("./game");
	write_solution_config(&root, "tsconfig.json", &references, true);
	write_solution_config(&join(&root, &["package"]), "tsconfig.json", &[], false);
	write_solution_config(&join(&root, &["game"]), "tsconfig.json", &["../package"], false);
	if sibling {
		write_solution_config(&join(&root, &["sibling"]), "tsconfig.json", &[], false);
	}
	(
		temp,
		join(&root, &["tsconfig.json"]),
		join(&root, &["package", "tsconfig.json"]),
		join(&root, &["game", "tsconfig.json"]),
	)
}
