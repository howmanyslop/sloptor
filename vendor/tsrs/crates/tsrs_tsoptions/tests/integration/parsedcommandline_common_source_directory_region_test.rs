use tsrs_core::arena::Region;
use tsrs_core::tspath::ComparePathsOptions;
use tsrs_core::{CompilerOptions, P};

use tsrs_tsoptions::parsedcommandline::new_parsed_command_line;

// The first `common_source_directory` call can come from a checker (a `rewriteRelativeImportExtensions` import into
// a referenced project) while its region is the allocation target; the rootDir errors it records are read later
// from the command line, which outlives that region.
#[test]
fn root_dir_errors_are_not_kept_in_the_asking_region() {
    let options = P::new(CompilerOptions { root_dir: "/lib".to_string(), ..Default::default() });
    let compare = ComparePathsOptions {
        use_case_sensitive_file_names: true,
        current_directory: "/".to_string(),
    };
    let config = P::new(new_parsed_command_line(
        options,
        vec!["/lib/a.ts".to_string(), "/shared/s.ts".to_string()],
        Vec::new(),
        compare,
    ));
    let region = Region::new(1 << 20);
    {
        let _scope = region.enter();
        assert_eq!(config.common_source_directory(), "/lib/");
    }
    let diagnostics = config.get_config_file_parsing_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(
        Region::containing(diagnostics[0].addr()).is_none_or(|r| !r.ptr_eq(&region)),
        "rootDir error allocated in the region"
    );
}
