//! `transform_build_lookup_test.go`
//!
//! Go's nil-project panic contract (`TestRootBuildClassLookupEmptyAndNilContract`'s second half)
//! has no Rust counterpart: a project is never nil.

use super::{temp_dir, with_class_analysis_input, with_class_analysis_input_in};
use crate::ErrorKind;
use crate::build_info::{BuildClass, BuildDecorator, BuildInfo};
use crate::plan::DecoratorPlan;
use crate::project::{Project, ProjectOptions, open_project};
use crate::state::FLAMEWORK_VERSION;
use crate::transform_analysis::analyze_flamework_classes;
use crate::transform_analysis_build::root_build_class_by_internal_id;

/// `decodeLookupBuild`
fn decode_lookup_build(fields: &str) -> BuildInfo {
	let mut info = BuildInfo::new("fixture/flamework.build", FLAMEWORK_VERSION);
	info.decode(format!(r#"{{"version":1,"flameworkVersion":"1.3.2",{fields}}}"#).as_bytes())
		.expect("decode");
	info
}

/// Go's `&Project{buildInfo: info}`: an opened fixture project carrying `info`.
fn project_with(info: BuildInfo) -> Project {
	let (_dir, directory) = temp_dir();
	std::fs::write(
		super::join(&[&directory, "package.json"]),
		r#"{"name":"fixture","version":"1.0.0"}"#,
	)
	.expect("write package.json");
	let mut project = open_project(&ProjectOptions {
		project_dir: directory,
		root_dir: "src".to_owned(),
		out_dir: "out".to_owned(),
		..ProjectOptions::default()
	})
	.expect("open project");
	project.build_info = std::sync::Arc::new(info);
	project
}

fn marshal(info: &BuildInfo) -> Vec<u8> {
	info.marshal_ordered_json().expect("marshal")
}

fn class(internal_id: &str, file_path: &str, decorators: &[(&str, &str)]) -> BuildClass {
	BuildClass {
		internal_id: internal_id.to_owned(),
		file_path: file_path.to_owned(),
		decorators: decorators
			.iter()
			.map(|(name, internal_id)| BuildDecorator {
				name: (*name).to_owned(),
				internal_id: (*internal_id).to_owned(),
			})
			.collect(),
	}
}

#[test]
fn root_build_class_lookup_selection() {
	let tests: [(&str, &str, &str, Option<BuildClass>); 8] = [
		("absent classes", r#""identifiers":{}"#, "a", None),
		("empty classes", r#""identifiers":{},"classes":[]"#, "", None),
		(
			"missing",
			r#""identifiers":{},"classes":[{"internalId":"a"}]"#,
			"b",
			None,
		),
		(
			"zero class is found",
			r#""identifiers":{},"classes":[{}]"#,
			"",
			Some(BuildClass::default()),
		),
		(
			"sorted mapped",
			r#""identifiers":{"key":"b"},"classes":[{"internalId":"a"},{"internalId":"b","filePath":"b.lua","decorators":[]}]"#,
			"key",
			Some(class("b", "b.lua", &[])),
		),
		(
			"present empty mapping wins",
			r#""identifiers":{"a":""},"classes":[{"internalId":"","filePath":"empty.lua"},{"internalId":"a"}]"#,
			"a",
			Some(class("", "empty.lua", &[])),
		),
		(
			"linear fallback",
			r#""identifiers":{},"classes":[{"internalId":"b"},{"internalId":"a","filePath":"a.lua"}]"#,
			"a",
			Some(class("a", "a.lua", &[])),
		),
		// sort.Search on [b,a,b] selects the third b, not the first linear match.
		(
			"binary duplicate winner",
			r#""identifiers":{"key":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]"#,
			"key",
			Some(class("b", "third.lua", &[("Third", "third"), ("Extra", "extra")])),
		),
	];
	for (name, fields, query, want) in tests {
		let project = project_with(decode_lookup_build(fields));
		let before = marshal(&project.build_info);
		let got = root_build_class_by_internal_id(&project, query);
		assert_eq!(got, want, "{name}");
		assert_eq!(
			marshal(&project.build_info),
			before,
			"{name}: lookup changed persisted state"
		);
	}
	let mut info = BuildInfo::new("fixture/flamework.build", FLAMEWORK_VERSION);
	let error = info
		.decode(br#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":null}"#)
		.expect_err("decode() succeeded for null classes, want ErrInvalidBuildInfo");
	assert!(error.is(ErrorKind::InvalidBuildInfo), "{error}");
	assert_eq!(
		error.message(),
		"flamework: invalid flamework.build: classes must not be null"
	);
}

#[test]
fn root_build_class_lookup_visibility_and_ownership() {
	let mut info = decode_lookup_build(
		r#""identifiers":{"mapped":"root"},"classes":[{"internalId":"child-key","filePath":"root-collision.lua"},{"internalId":"root","filePath":"root.lua","decorators":[{"name":"Root","internalId":"root-decorator"}]}]"#,
	);
	let mut child = decode_lookup_build(
		r#""identifiers":{"child-key":"child-target","mapped":"child-target"},"classes":[{"internalId":"child-target","filePath":"child.lua"},{"internalId":"root","filePath":"child-collision.lua"}]"#,
	);
	let grandchild =
		decode_lookup_build(r#""identifiers":{"grand-key":"grand-target"},"classes":[{"internalId":"grand-target"}]"#);
	child.packages = vec![grandchild];
	info.packages = vec![child];
	let mut project = project_with(info);
	for (query, path) in [
		("child-key", "root-collision.lua"),
		("root", "root.lua"),
		("mapped", "root.lua"),
	] {
		let got = root_build_class_by_internal_id(&project, query);
		assert_eq!(
			got.map(|class| class.file_path),
			Some(path.to_owned()),
			"root lookup {query:?}"
		);
	}
	for query in ["child-target", "grand-key", "grand-target"] {
		let got = root_build_class_by_internal_id(&project, query);
		assert!(got.is_none(), "child-only lookup {query:?} escaped root: {got:?}");
	}
	let previous = root_build_class_by_internal_id(&project, "mapped").expect("mapped");
	let mut returned = root_build_class_by_internal_id(&project, "mapped").expect("mapped");
	returned.decorators[0].name = "returned mutation".to_owned();
	let root_class = |project: &Project| project.build_info.classes.as_ref().expect("classes")[1].clone();
	assert!(
		previous.decorators[0].name == "Root" && root_class(&project).decorators[0].name == "Root",
		"returned decorators alias the root or another result"
	);
	project.build_info_mut().classes.as_mut().expect("classes")[1].decorators[0].name = "current".to_owned();
	let got = root_build_class_by_internal_id(&project, "mapped").expect("mapped");
	assert!(
		got.decorators[0].name == "current" && previous.decorators[0].name == "Root",
		"lookup is stale or root mutation changed a previous result"
	);
	project
		.build_info_mut()
		.identifiers
		.values
		.insert("mapped".to_owned(), "child-key".to_owned());
	let got = root_build_class_by_internal_id(&project, "mapped");
	assert_eq!(
		got.map(|class| class.file_path),
		Some("root-collision.lua".to_owned()),
		"mapping mutation ignored"
	);
	project.build_info_mut().identifiers.values.remove("mapped");
	assert!(
		root_build_class_by_internal_id(&project, "mapped").is_none(),
		"deleted mapping was retained"
	);
	let other = project_with(decode_lookup_build(
		r#""identifiers":{"mapped":"root"},"classes":[{"internalId":"root","filePath":"other.lua"}]"#,
	));
	project.build_info_mut().classes = Some(vec![class("root", "replacement.lua", &[])]);
	let got = root_build_class_by_internal_id(&project, "root");
	assert_eq!(
		got.map(|class| class.file_path),
		Some("replacement.lua".to_owned()),
		"class replacement ignored"
	);
	let got = root_build_class_by_internal_id(&other, "mapped");
	assert_eq!(
		got.map(|class| class.file_path),
		Some("other.lua".to_owned()),
		"same-path Projects shared state"
	);
}

#[test]
fn root_build_class_lookup_empty_and_nil_contract() {
	for classes in [None, Some(Vec::new())] {
		let mut info = BuildInfo::new("fixture/flamework.build", FLAMEWORK_VERSION);
		info.classes = classes;
		let project = project_with(info);
		assert_eq!(root_build_class_by_internal_id(&project, ""), None);
	}
}

#[test]
fn root_build_class_lookup_ignores_unrelated_metadata() {
	let mut info = decode_lookup_build(
		r#""identifiers":{"key":"a"},"metadata":{"config":{"logLevel":"verbose","extra":{"nested":[1,2]}},"globs":{"paths":{"pattern":["a.ts","b.ts"]}}},"classes":[{"internalId":"a","decorators":[]}]"#,
	);
	// Snapshot silently omits invalid in-memory runtime config. The lookup must still return the
	// selected class without changing or validating that config. Go also sets an invalid log
	// level, which the Rust enum cannot hold.
	info.metadata
		.as_mut()
		.and_then(|metadata| metadata.config.as_mut())
		.expect("config")
		.additional
		.insert("invalid".to_owned(), b"{".to_vec());
	let project = project_with(info);
	let got = root_build_class_by_internal_id(&project, "key");
	assert_eq!(got, Some(class("a", "", &[])), "lookup with unrelated invalid config");
	let additional = &project
		.build_info
		.metadata
		.as_ref()
		.and_then(|metadata| metadata.config.as_ref())
		.expect("config")
		.additional;
	assert_eq!(
		additional.get("invalid").map(Vec::as_slice),
		Some(&b"{"[..]),
		"lookup changed unrelated runtime config"
	);
}

#[test]
fn discover_flamework_classes_decoded_lookup_payload() {
	with_class_analysis_input(
		&[
			("package.json", r#"{"name":"fixture-package","version":"1.0.0"}"#),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("src/service.ts", "export class Service {}"),
		],
		"src/service.ts",
		|input| {
			input.project.build_info = std::sync::Arc::new(decode_lookup_build(
				r#""identifiers":{"fixture-package:out/service@Service":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]"#,
			));
			let before = marshal(&input.project.build_info);
			for _ in 0..2 {
				let analysis = analyze_flamework_classes(input).expect("AnalyzeFlameworkClasses");
				let files = analysis.files();
				assert!(files.len() == 1 && files[0].classes.len() == 1, "analysis = {files:?}");
				let mut plan = files[0].classes[0].clone();
				let want = vec![
					DecoratorPlan {
						name: "Third".to_owned(),
						internal_id: "third".to_owned(),
					},
					DecoratorPlan {
						name: "Extra".to_owned(),
						internal_id: "extra".to_owned(),
					},
				];
				assert!(
					plan.decorators == want && !plan.contains_legacy_decorator,
					"persisted decorators/provenance = {plan:?}"
				);
				assert_eq!(analysis.classes()[0].decorator_ids, ["third", "extra"]);
				plan.decorators[0].name = "returned mutation".to_owned();
			}
			assert_eq!(
				marshal(&input.project.build_info),
				before,
				"fallback changed class/identifier insertion order"
			);
		},
	);
}

#[test]
fn discover_flamework_classes_lookup_failure_boundaries() {
	let tests = [
		(
			"decorator before lookup",
			r#"import { Decorator } from "external"; @Decorator() export class Service {}"#,
			r#"{"types":"index.d.ts"}"#,
			"invalid Flamework package.json: declaration package",
		),
		(
			"constructor after lookup",
			"export class Service<T extends string> { constructor(value: T extends string ? 1 : 2) {} }",
			r#"{"name":"external","types":"index.d.ts"}"#,
			r#"Could not find UID for type "T extends string ? 1 : 2""#,
		),
	];
	for (name, source, package_json, want) in tests {
		with_class_analysis_input(
			&[
				("package.json", r#"{"name":"fixture-package"}"#),
				(
					"tsconfig.json",
					r#"{"compilerOptions":{"experimentalDecorators":true,"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
				),
				("src/service.ts", source),
				("node_modules/external/package.json", package_json),
				(
					"node_modules/external/index.d.ts",
					r#"export declare function Decorator(): { (target: unknown): void; _flamework_Decorator: "Class" };"#,
				),
				(
					"flamework.build",
					r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":[{"internalId":"fixture-package:out/service@Service"}]}"#,
				),
			],
			"src/service.ts",
			|input| {
				let before = marshal(&input.project.build_info);
				let error = analyze_flamework_classes(input).err();
				assert!(
					error.as_ref().is_some_and(|error| error.message().contains(want)),
					"{name}: analysis error = {error:?}, want {want:?}"
				);
				assert_eq!(
					marshal(&input.project.build_info),
					before,
					"{name}: failed discovery changed metadata"
				);
			},
		);
	}
}

#[test]
fn discover_flamework_classes_package_root_lookup() {
	for local in [false, true] {
		let mut files = vec![
			("package.json", r#"{"name":"fixture-package"}"#),
			(
				"flamework.build",
				r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"fixture-package:project/out/service@Service":"parent"},"classes":[{"internalId":"parent","decorators":[{"name":"Parent","internalId":"parent-decorator"}]}]}"#,
			),
			(
				"project/tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
			),
			("project/src/service.ts", "export class Service {}"),
		];
		if local {
			// A present empty project file takes precedence over package-root data.
			files.push((
				"project/flamework.build",
				r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":[]}"#,
			));
		}
		with_class_analysis_input_in(&files, "project", "src/service.ts", |input| {
			let analysis = analyze_flamework_classes(input).expect("AnalyzeFlameworkClasses");
			if local {
				assert!(
					analysis.classes().is_empty(),
					"local empty metadata leaked package fallback: {:?}",
					analysis.classes()
				);
				return;
			}
			let classes = analysis.classes();
			assert!(
				classes.len() == 1 && classes[0].decorator_ids == ["parent-decorator"],
				"package-root fallback = {classes:?}"
			);
			let plan = &analysis.files()[0].classes[0];
			assert!(
				!plan.contains_legacy_decorator && plan.decorators[0].name == "Parent",
				"package-root payload/provenance = {plan:?}"
			);
		});
	}
}
