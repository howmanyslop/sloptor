use std::io::Read;
use std::path::Path;
use std::process::Command;

use tsrs_contentmapper::spawn_process;

use crate::path_test::TempDir;

const TEST: &str = "process_windows_test::test_windows_executable_lookup";
const CASE: &str = "TSRS_MAPPER_LOOKUP_CASE";
const CHILD: &str = "mapper-child";

#[test]
fn test_windows_executable_lookup() {
    if std::env::args().next_back().as_deref() == Some(CHILD) {
        println!("mapper executable: {}", std::env::current_exe().unwrap().display());
        return;
    }
    if let Ok(case) = std::env::var(CASE) {
        run_case(&case);
        return;
    }

    let temp = TempDir::new();
    let runtime = temp.0.join("runtime with spaces");
    let mapper = temp.0.join("mapper");
    let relative_runtime = mapper.join("bin");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::create_dir_all(&relative_runtime).unwrap();
    let exe = std::env::current_exe().unwrap();
    for path in [
        runtime.join("mapper-runner.exe"),
        runtime.join("mapper-runner.custom"),
        relative_runtime.join("mapper-runner.exe"),
    ] {
        std::fs::copy(&exe, path).unwrap();
    }

    // Each environment runs alone in a child; the surrounding integration binary never changes PATH or PATHEXT.
    for (case, pathext) in [
        ("bare-default", None),
        ("bare-empty", Some("")),
        ("bare-exe", Some(";.EXE;;")),
        ("bare-custom", Some("CUSTOM;.EXE")),
        ("explicit-extension", Some(".CUSTOM")),
        ("absolute", None),
        ("relative", None),
        ("missing", Some(".CUSTOM")),
        ("relative-path", Some(".EXE")),
    ] {
        let mut command = Command::new(&exe);
        command
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .current_dir(&temp.0)
            .env(CASE, case)
            .env("NoDefaultCurrentDirectoryInExePath", "1");
        if matches!(case, "absolute" | "relative") {
            command.env("PATH", "");
        } else if case == "relative-path" {
            command.env("PATH", "runtime with spaces");
        } else {
            command.env("PATH", &runtime);
        }
        if let Some(pathext) = pathext {
            command.env("PATHEXT", pathext);
        } else {
            command.env_remove("PATHEXT");
        }
        let output = command.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{case}:\n{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("1 passed;"), "the child must run {TEST}: {stdout}");
    }
}

fn run_case(case: &str) {
    let cwd = std::env::current_dir().unwrap();
    let name = match case {
        "explicit-extension" => "mapper-runner.exe".to_string(),
        "absolute" => cwd.join("runtime with spaces/mapper-runner.exe").display().to_string(),
        "relative" => r".\bin\mapper-runner.exe".to_string(),
        "missing" => "missing-mapper".to_string(),
        _ => "mapper-runner".to_string(),
    };
    let command = [
        name,
        TEST.to_string(),
        "--exact".to_string(),
        "--nocapture".to_string(),
        CHILD.to_string(),
    ];
    let dir = cwd.join("mapper");
    let result = spawn_process(&command, dir.to_str().unwrap(), None);
    if matches!(case, "missing" | "relative-path") {
        let error = result.err().expect("lookup must fail before launching the mapper");
        let reason = if case == "missing" {
            "executable file not found in %PATH%"
        } else {
            "cannot run executable found relative to current directory"
        };
        assert_eq!(error, format!("exec: \"{}\": {reason}", command[0]));
        return;
    }
    let mut process = result.unwrap_or_else(|error| panic!("{case}: {error}"));
    let mut output = String::new();
    process.reader.read_to_string(&mut output).unwrap();
    process.closer.close().unwrap();
    assert!(output.contains("1 passed;"), "the mapper child did not run: {output}");
    let executable = output
        .lines()
        .find_map(|line| line.split_once("mapper executable: ").map(|(_, path)| path))
        .expect("the child printed its executable");
    let expected = if case == "bare-custom" { "mapper-runner.custom" } else { "mapper-runner.exe" };
    assert_eq!(Path::new(executable).file_name().unwrap(), expected);
}
