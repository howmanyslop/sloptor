use std::fmt::Write as _;
use std::process::Command;
use std::sync::Arc;

use tsrs_compiler::{
    CompilerHost, Program, ProgramOptions, new_compiler_host, new_program, worker_pool,
};
use tsrs_core::P;
use tsrs_core::tspath::Path;
use tsrs_tsoptions::{self as tsoptions, ParseConfigHost};
use tsrs_vfs::{FS, vfstest};

const CHILD: &str = "TSRS_ARENA_LEAK_889_TEST";

/// Runs `test` alone in a child process of this integration test binary.
/// Returns true in the child, false in the parent after the child passes.
fn in_child(test: &str) -> bool {
    if std::env::var_os(CHILD).is_some_and(|name| name == test) {
        return true;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .envs(std::env::vars())
        .env("RAYON_NUM_THREADS", "2")
        .env(CHILD, test)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("1 passed;"), "the child must run the isolated case:\n{stdout}");
    false
}

struct ParseConfigHostImpl {
    fs: Arc<dyn FS>,
}

impl ParseConfigHost for ParseConfigHostImpl {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

fn host_for(files: &[(&str, &str)]) -> Arc<dyn CompilerHost> {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().map(|&(k, v)| (k, v)), true));
    new_compiler_host("/", fs, "", None, None, None)
}

fn config_for(files: &[(&str, &str)]) -> P<tsoptions::ParsedCommandLine> {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().map(|&(k, v)| (k, v)), true));
    let host: &'static ParseConfigHostImpl = Box::leak(Box::new(ParseConfigHostImpl { fs }));
    let (config, diagnostics) = tsoptions::get_parsed_command_line_of_config_file(
        "/src/tsconfig.json",
        None,
        None,
        host,
        None,
    );
    assert_eq!(diagnostics, [] as [tsrs_core::P<tsrs_ast::Diagnostic>; 0]);
    P::new(config.unwrap())
}

fn total_worker_arena_bytes() -> usize {
    worker_pool().broadcast(|_| tsrs_core::ptr::arena_used_bytes()).into_iter().sum()
}

fn make_files(flipped: bool) -> Vec<(&'static str, &'static str)> {
    let mut files: Vec<(&'static str, &'static str)> = vec![(
        "/src/tsconfig.json",
        r#"{"compilerOptions":{"noLib":true,"module":"commonjs"},"files":["index.ts"]}"#,
    )];
    let mut index = String::new();
    for i in 0..64 {
        let specifier = if i == 0 && flipped { "./x0alt" } else { "./x" };
        let _ = writeln!(index, "import {{ v }} from '{specifier}{i}';");
        files.push((Box::leak(format!("/src/x{i}.ts").into_boxed_str()), "export const v = 1;"));
    }
    files.push(("/src/x0alt.ts", "export const v = 2;"));
    files.push((
        Box::leak("/src/index.ts".to_string().into_boxed_str()),
        Box::leak(index.into_boxed_str()),
    ));
    files
}

#[test]
fn update_program_full_rebuild_uses_regions() {
    let test_name = "arena_leak_889_test::update_program_full_rebuild_uses_regions";
    if !in_child(test_name) {
        return;
    }

    let initial_files = make_files(false);
    let mut opts = ProgramOptions::new(config_for(&initial_files), host_for(&initial_files));
    opts.regions = Some(Arc::default());
    let program: &'static Program = new_program(opts);
    let baseline = total_worker_arena_bytes();

    let mut current = program;
    for i in 0..30 {
        let flipped = i % 2 == 0;
        let files = make_files(flipped);
        let host = host_for(&files);
        let (updated, _new_file, reused) =
            current.update_program(&Path::from("/src/index.ts"), host, None, None);
        assert!(!reused, "iteration {i} should fall back to a full rebuild");
        current = updated;
    }

    let after = total_worker_arena_bytes();
    assert_eq!(
        after,
        baseline,
        "worker thread arenas grew by {} bytes across full rebuilds",
        after.wrapping_sub(baseline)
    );
}
