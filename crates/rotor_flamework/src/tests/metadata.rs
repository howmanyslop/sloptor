//! `metadata_test.go`

use crate::metadata::Metadata;
use crate::plan::{AnalysisState, ClassPlan, FileAnalysis};

#[test]
fn metadata_requests_when_wildcard_and_exclusion_are_present() {
	let metadata = Metadata::new(["*", "~guard", "macro"]);
	let got = [
		metadata.requested("guard"),
		metadata.requested("macro"),
		metadata.requested("constructor"),
	];
	assert_eq!(got, [false, true, true]);
}

#[test]
fn metadata_tokens_when_whitespace_contains_runs() {
	let metadata = Metadata::parse_text("  macro\tconstructor\nproperties  ");
	let mut got = metadata.tokens();
	assert_eq!(got, ["constructor\nproperties", "macro"]);
	"mutated".clone_into(&mut got[0]);
	assert_ne!(metadata.tokens(), got);
}

#[test]
fn analysis_plans_when_inputs_are_unordered() {
	let mut state = AnalysisState::new();
	let inputs = [
		FileAnalysis {
			file_id: "src/z.ts".to_owned(),
			metadata: Metadata::new(["macro"]),
			classes: vec![ClassPlan {
				internal_id: "pkg:z@Z".to_owned(),
				..ClassPlan::default()
			}],
		},
		FileAnalysis {
			file_id: "src/a.ts".to_owned(),
			metadata: Metadata::new(["constructor"]),
			classes: vec![ClassPlan {
				internal_id: "pkg:a@A".to_owned(),
				..ClassPlan::default()
			}],
		},
	];
	let plans = state.analyze(&inputs).unwrap();
	assert_eq!([plans[0].file_id(), plans[1].file_id()], ["src/a.ts", "src/z.ts"]);
	let mut classes = plans[0].classes();
	"mutated".clone_into(&mut classes[0].internal_id);
	assert_eq!(state.plans()[0].classes()[0].internal_id, "pkg:a@A");
}

#[test]
fn analysis_plans_when_file_is_duplicated() {
	let mut state = AnalysisState::new();
	let inputs = [
		FileAnalysis {
			file_id: "src/a.ts".to_owned(),
			..FileAnalysis::default()
		},
		FileAnalysis {
			file_id: "src/./a.ts".to_owned(),
			..FileAnalysis::default()
		},
	];
	assert!(state.analyze(&inputs).is_err());
}
