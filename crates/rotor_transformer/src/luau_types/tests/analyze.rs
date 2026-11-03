//! `luautypes_analyze_test.go`: fixtures under `testdata/luautypes` are rendered with the option
//! on and handed to a real Luau analyzer. The tests return early (Go skips them) when neither
//! `luau-lsp` nor `luau-analyze` is installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{luau_types_fixtures, render_luau_types};
use crate::tests::transformer_testdata;

/// `luauAnalyzerKind`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyzerKind {
	/// `luau-lsp analyze`, the only one of the two that loads a definitions file.
	Lsp,
	/// The plain `luau-analyze` binary.
	Analyze,
}

/// `luauAnalyzer`: a resolved analyzer binary plus the kind that decides its command line.
#[derive(Clone, Debug)]
pub struct Analyzer {
	path: PathBuf,
	kind: AnalyzerKind,
}

const LUAU_MODULE_NAME: &str = "module.luau";
const LUAU_STUB_NAME: &str = "sloptor.d.luau";
const LUAU_STRICT_CONFIG: &str = "{\"languageMode\": \"strict\"}\n";

/// `findLuauAnalyzer`: luau-lsp first, then luau-analyze; `None` (Go skips) when neither is
/// installed.
pub fn find_luau_analyzer() -> Option<Analyzer> {
	if let Some(path) = lookup_luau_tool("luau-lsp") {
		return Some(Analyzer {
			path,
			kind: AnalyzerKind::Lsp,
		});
	}
	if let Some(path) = lookup_luau_tool("luau-analyze") {
		return Some(Analyzer {
			path,
			kind: AnalyzerKind::Analyze,
		});
	}
	println!("no Luau analyzer found; install one with `mise install luau-lsp` (preferred) or `mise install luau`");
	None
}

/// `lookupLuauTool`: `name` on `PATH`, then through `mise which name`.
fn lookup_luau_tool(name: &str) -> Option<PathBuf> {
	if let Some(paths) = std::env::var_os("PATH") {
		for directory in std::env::split_paths(&paths) {
			let candidate = directory.join(name);
			if candidate.is_file() {
				return Some(candidate);
			}
		}
	}
	let output = Command::new("mise").args(["which", name]).output().ok()?;
	if !output.status.success() {
		return None;
	}
	let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
	path.is_file().then_some(path)
}

/// `luauStubPath`
fn luau_stub_path() -> PathBuf {
	transformer_testdata("luautypes")
		.join(LUAU_STUB_NAME)
		.canonicalize()
		.expect("the stub exists")
}

/// `analyzeLuauStrict`: every type or syntax error the analyzer reports for `source` in
/// strict mode. `None` when luau-analyze cannot judge a module that reaches a stubbed global
/// (Go skips).
pub fn analyze_luau_strict(analyzer: &Analyzer, source: &str, with_stub: bool) -> Option<Vec<String>> {
	let directory = tempfile::tempdir().expect("a temporary directory");
	std::fs::write(directory.path().join(LUAU_MODULE_NAME), source).expect("the module writes");
	std::fs::write(directory.path().join(".luaurc"), LUAU_STRICT_CONFIG).expect("the config writes");

	let mut args: Vec<String> = match analyzer.kind {
		AnalyzerKind::Lsp => {
			let mut args = vec!["analyze".to_owned(), "--flag:LuauSolverV2=true".to_owned()];
			if with_stub {
				args.push(format!("--definitions={}", luau_stub_path().display()));
			} else {
				args.push("--platform=standard".to_owned());
			}
			args
		}
		AnalyzerKind::Analyze => vec!["--mode=strict".to_owned()],
	};
	args.push(LUAU_MODULE_NAME.to_owned());

	let output = Command::new(&analyzer.path)
		.args(&args)
		.current_dir(directory.path())
		.output()
		.expect("the analyzer runs");
	let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
	text.push_str(&String::from_utf8_lossy(&output.stderr));
	let failed = !output.status.success();
	assert!(
		!failed || output.status.code() == Some(1),
		"{} {}: {:?}\n{text}",
		analyzer.path.display(),
		args.join(" "),
		output.status
	);
	assert!(
		!text.contains("[ERROR]"),
		"{} reported an error:\n{text}",
		analyzer.path.display()
	);

	let findings: Vec<String> = text
		.lines()
		.filter(|line| line.contains("TypeError") || line.contains("SyntaxError"))
		.map(str::to_owned)
		.collect();
	assert!(
		!failed || !findings.is_empty() || text.contains(&format!("{LUAU_MODULE_NAME}(")),
		"{} exited 1 without reporting on {LUAU_MODULE_NAME}:\n{text}",
		analyzer.path.display()
	);
	if with_stub
		&& analyzer.kind == AnalyzerKind::Analyze
		&& let Some(name) = unknown_stub_global(&findings)
	{
		println!(
			"module reaches the stubbed global {name:?} and luau-analyze cannot load {LUAU_STUB_NAME}; install luau-lsp"
		);
		return None;
	}
	Some(findings)
}

/// `unknownStubGlobal`: the first unknown global or type among `findings` that the stub
/// declares.
fn unknown_stub_global(findings: &[String]) -> Option<String> {
	let stub = std::fs::read_to_string(luau_stub_path()).expect("the stub reads");
	let declared: Vec<String> = stub
		.lines()
		.filter_map(|line| {
			let rest = line.strip_prefix("declare ")?;
			let rest = rest.strip_prefix("extern type ").unwrap_or(rest);
			let name: String = rest
				.chars()
				.take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
				.collect();
			(!name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then_some(name)
		})
		.collect();
	for finding in findings {
		for marker in ["Unknown global '", "Unknown type '"] {
			if let Some(index) = finding.find(marker) {
				let name: String = finding[index + marker.len()..]
					.chars()
					.take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
					.collect();
				if declared.contains(&name) {
					return Some(name);
				}
			}
		}
	}
	None
}

/// `luauErrorLines`: the sorted set of line numbers the findings point at. A finding without a
/// position fails the test.
pub fn luau_error_lines(findings: &[String]) -> Vec<usize> {
	let mut lines = Vec::new();
	for finding in findings {
		let line = diagnostic_line(finding)
			.unwrap_or_else(|| panic!("diagnostic without a position in {LUAU_MODULE_NAME}: {finding}"));
		if !lines.contains(&line) {
			lines.push(line);
		}
	}
	lines.sort_unstable();
	lines
}

/// `luauDiagnosticPosition`: the line of `(<line>,<col>): TypeError: ` or `SyntaxError`.
fn diagnostic_line(finding: &str) -> Option<usize> {
	for marker in ["): TypeError: ", "): SyntaxError: "] {
		let Some(end) = finding.find(marker) else {
			continue;
		};
		let head = &finding[..end];
		let open = head.rfind('(')?;
		let (line, column) = head[open + 1..].split_once(',')?;
		if column.chars().all(|c| c.is_ascii_digit()) && !column.is_empty() {
			return line.parse().ok();
		}
	}
	None
}

/// The `.luau` files directly under `dir`, sorted.
fn luau_files(dir: &Path) -> Vec<PathBuf> {
	let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
		.expect("the directory reads")
		.filter_map(|entry| entry.ok().map(|entry| entry.path()))
		.filter(|path| path.extension().is_some_and(|extension| extension == "luau"))
		.collect();
	files.sort();
	files
}

#[test]
fn luau_types_class_idiom_probes() {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	let expected: &[(&str, &[usize])] = &[
		// P1f: `local bad1: string = f:get()`, `f.y`, `Foo.new("s")`.
		("class-basic.luau", &[20, 21, 22]),
		// P3b: generic class with `self:constructor(value) or self`;
		// `local bad: string = Box.new(1):get()`.
		("class-generic.luau", &[20]),
		// P2h: flat derived alias; `local bad: Derived = Base.new(1)`,
		// `local bad2: string = d:get()`, `b:both()` on the base.
		("class-inherit-flat.luau", &[34, 35, 36]),
		// P2k: generic intersection alias; `local bad: string = p:get()`,
		// `local bad2: Box<string> = p`.
		("class-inherit-generic.luau", &[35, 36]),
		// P5b: statics as `self: typeof(Foo)`; the three P1f errors plus `Foo.make(f, 2)`
		// passing an instance where the class table goes.
		("class-statics.luau", &[28, 29, 30, 31]),
		// P6: `local bad: Direction = "sideways"`.
		("enum-string.luau", &[6]),
		// P9b: colon-form methods leave `self` untyped, so `self.nope` on line 19 is silently
		// accepted and only the arithmetic over two `unknown` method results on line 22 is
		// rejected.
		("method-colon-self.luau", &[22]),
		// P7e: `type function` with `T: type` parameters, generic alias defaults, `read`/`write`
		// members, `keyof<>`/`index<>`, packs.
		("type-function-param-annotated.luau", &[]),
	];
	let mut failures = Vec::new();
	let mut seen = Vec::new();
	for probe in luau_files(&transformer_testdata("luautypes").join("probes")) {
		let name = probe
			.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or_default()
			.to_owned();
		let Some(&(_, want)) = expected.iter().find(|(expected, _)| *expected == name) else {
			failures.push(format!("probe {name} has no expected-error entry"));
			continue;
		};
		seen.push(name.clone());
		let source = std::fs::read_to_string(&probe).expect("the probe reads");
		let Some(findings) = analyze_luau_strict(&analyzer, &source, false) else {
			continue;
		};
		let got = luau_error_lines(&findings);
		if got != want {
			failures.push(format!(
				"{name}: error lines: got {got:?}, want {want:?}\n{}",
				findings.join("\n")
			));
		}
	}
	for (name, _) in expected {
		if !seen.iter().any(|seen| seen == name) {
			failures.push(format!(
				"expected probe {name} is missing from testdata/luautypes/probes"
			));
		}
	}
	assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The stub actually reaches the analyzer: with it loaded only the deliberate error survives,
/// and without it the same module trips over the globals the stub declares.
#[test]
fn luau_types_stub_definitions_honored() {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	if analyzer.kind != AnalyzerKind::Lsp {
		println!("luau-analyze cannot load a definitions file; install luau-lsp");
		return;
	}
	let source = [
		"--!strict",
		"local TS = _G[script]",
		"local origin: Vector3 = Vector3.new(1, 2, 3)",
		"local name: string = script.Name",
		"local bad: string = origin.X",
		"return TS.import(script, name)",
		"",
	]
	.join("\n");

	let with_stub = analyze_luau_strict(&analyzer, &source, true).expect("luau-lsp judges every module");
	assert_eq!(
		luau_error_lines(&with_stub),
		vec![5],
		"with the stub only line 5 should fail:\n{}",
		with_stub.join("\n")
	);

	let without_stub = analyze_luau_strict(&analyzer, &source, false).expect("luau-lsp judges every module");
	for want in ["Unknown global 'script'", "Unknown type 'Vector3'"] {
		assert!(
			without_stub.iter().any(|line| line.contains(want)),
			"without the stub expected {want:?} in:\n{}",
			without_stub.join("\n")
		);
	}
}

/// Every fixture under `testdata/luautypes/src`, rendered with `luauTypes` on, analyzes clean
/// in strict mode.
#[test]
#[ignore = "needs slice vi: export declarations"]
fn luau_types_fixtures_analyze_clean() {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	let mut failures = Vec::new();
	for rel_path in luau_types_fixtures() {
		let rendered = render_luau_types(&rel_path, true);
		let Some(findings) = analyze_luau_strict(&analyzer, &rendered, true) else {
			continue;
		};
		if !findings.is_empty() {
			failures.push(format!(
				"{rel_path} does not analyze clean:\n{}\n--- rendered ---\n{rendered}",
				findings.join("\n")
			));
		}
	}
	assert!(failures.is_empty(), "{}", failures.join("\n"));
}
