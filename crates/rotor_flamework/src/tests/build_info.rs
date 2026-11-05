//! `build_info_test.go`

use super::{join, temp_dir};
use crate::ErrorKind;
use crate::build_info::{BuildClass, BuildDecorator, BuildInfo};
use crate::runtime_config::{RuntimeConfig, RuntimeLogLevel};
use crate::state::FLAMEWORK_VERSION;

#[test]
fn build_info_marshal_when_fields_are_added_out_of_lexical_order() {
	let mut info = BuildInfo::new("flamework.build", "1.3.2");
	info.add_identifier("z.internal", "id-z").unwrap();
	info.add_identifier("a.internal", "id-a").unwrap();
	info.add_class(&BuildClass {
		file_path: "out/z.lua".to_owned(),
		internal_id: "class-z".to_owned(),
		decorators: vec![BuildDecorator {
			name: "Service".to_owned(),
			internal_id: "decor-z".to_owned(),
		}],
	})
	.unwrap();
	info.set_runtime_config(Some(&RuntimeConfig {
		profiling: Some(true),
		log_level: Some(RuntimeLogLevel::Verbose),
		..RuntimeConfig::default()
	}));
	let first = info.marshal_ordered_json().unwrap();
	let second = info.marshal_ordered_json().unwrap();
	let want = "{\n\t\"version\": 1,\n\t\"flameworkVersion\": \"1.3.2\",\n\t\"identifiers\": {\n\t\t\"z.internal\": \"id-z\",\n\t\t\"a.internal\": \"id-a\"\n\t},\n\t\"classes\": [\n\t\t{\n\t\t\t\"filePath\": \"out/z.lua\",\n\t\t\t\"internalId\": \"class-z\",\n\t\t\t\"decorators\": [\n\t\t\t\t{\n\t\t\t\t\t\"name\": \"Service\",\n\t\t\t\t\t\"internalId\": \"decor-z\"\n\t\t\t\t}\n\t\t\t]\n\t\t}\n\t],\n\t\"metadata\": {\n\t\t\"config\": {\n\t\t\t\"logLevel\": \"verbose\",\n\t\t\t\"profiling\": true\n\t\t}\n\t}\n}";
	assert_eq!(String::from_utf8(first).unwrap(), want);
	assert_eq!(String::from_utf8(second).unwrap(), want);
}

#[test]
fn load_build_info_when_input_is_corrupt() {
	for data in [r#"{"version":1"#, r#"{"version":1,"flameworkVersion":"1.3.2"}"#] {
		let (_dir, root) = temp_dir();
		let path = join(&[&root, "flamework.build"]);
		std::fs::write(&path, data).unwrap();
		let error = BuildInfo::load(&path, "1.3.2").unwrap_err();
		assert!(error.is(ErrorKind::InvalidBuildInfo), "{error}");
	}
}

#[test]
fn build_info_packages_when_flamework_versions_differ() {
	let (_dir, root_dir) = temp_dir();
	let path = join(&[&root_dir, "flamework.build"]);
	std::fs::write(
		&path,
		r#"{"version":1,"flameworkVersion":"9.9.9","identifiers":{"dep":"id"}}"#,
	)
	.unwrap();
	let mut root = BuildInfo::new("root/flamework.build", FLAMEWORK_VERSION);
	let child = BuildInfo::load(&path, FLAMEWORK_VERSION).unwrap();
	root.add_package(child);
	assert_eq!(root.identifier("dep").as_deref(), Some("id"));
}

#[test]
fn load_build_info_when_schema_permits_future_properties() {
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "flamework.build"]);
	std::fs::write(
		&path,
		r#"{"version":1.5,"flameworkVersion":"1.3.2","identifiers":{},"future":true,"classes":[{}]}"#,
	)
	.unwrap();
	let info = BuildInfo::load(&path, "1.3.2").unwrap();
	let data = String::from_utf8(info.marshal_ordered_json().unwrap()).unwrap();
	for fragment in [r#""version": 1.5"#, r#""future": true"#, r#""classes": ["#] {
		assert!(
			data.contains(fragment),
			"encoded build info omitted {fragment}:\n{data}"
		);
	}
}

#[test]
fn build_info_hash_string_when_key_repeats() {
	let mut info = BuildInfo::new("flamework.build", "1.3.2");
	let first = info.hash_string("text", "context").unwrap();
	let second = info.hash_string("text", "context").unwrap();
	assert_eq!(first, second);
	assert_eq!(info.string_hash("context:text"), Some(first.as_str()));
}

#[test]
fn build_info_packages_when_nested_packages_overlap() {
	let mut root = BuildInfo::new("root/flamework.build", "1.3.2");
	let mut first = BuildInfo::new("first/flamework.build", "1.3.2");
	first.set_identifier_prefix(Some("pkg-first"));
	first.add_identifier("shared", "first").unwrap();
	let mut grandchild = BuildInfo::new("grand/flamework.build", "1.3.2");
	grandchild.set_identifier_prefix(Some("pkg-grand"));
	grandchild.add_identifier("deep", "grand").unwrap();
	first.add_package(grandchild);
	let mut second = BuildInfo::new("second/flamework.build", "1.3.2");
	second.set_identifier_prefix(Some("pkg-second"));
	second.add_identifier("shared", "second").unwrap();
	root.add_package(first);
	root.add_package(second);

	assert_eq!(root.identifier("shared").as_deref(), Some("first"));
	assert_eq!(root.identifier("deep").as_deref(), Some("grand"));
	let mut packages = root.package_snapshots();
	let prefixes: Vec<&str> = packages
		.iter()
		.filter_map(|snapshot| snapshot.identifier_prefix())
		.collect();
	assert_eq!(prefixes, ["pkg-first", "pkg-grand", "pkg-second"]);
	packages[0]
		.identifiers
		.insert("shared".to_owned(), "mutated".to_owned());
	assert_eq!(root.identifier("shared").as_deref(), Some("first"));
}

#[test]
fn build_info_internal_identifier_when_identifier_belongs_to_package() {
	let mut root = BuildInfo::new("root/flamework.build", FLAMEWORK_VERSION);
	let mut child = BuildInfo::new("child/flamework.build", FLAMEWORK_VERSION);
	child.add_identifier("internal", "external").unwrap();
	root.add_package(child);
	assert_eq!(root.internal_identifier("external"), None);
	assert_eq!(root.internal_identifier("internal").as_deref(), Some("external"));
}

#[test]
fn build_info_optional_fields_when_empty_values_are_explicit() {
	let (_dir, root) = temp_dir();
	let path = join(&[&root, "flamework.build"]);
	std::fs::write(
		&path,
		r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"identifierPrefix":"","classes":[],"metadata":{"globs":{"paths":{},"origins":{}}}}"#,
	)
	.unwrap();
	let info = BuildInfo::load(&path, "1.3.2").unwrap();
	let data = String::from_utf8(info.marshal_ordered_json().unwrap()).unwrap();
	for fragment in [
		r#""identifierPrefix": """#,
		r#""classes": []"#,
		r#""paths": {}"#,
		r#""origins": {}"#,
	] {
		assert!(
			data.contains(fragment),
			"encoded build info omitted {fragment}:\n{data}"
		);
	}
}
