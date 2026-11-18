use std::sync::Arc;

use tsrs_compiler::{
    ProgramOptions, ProgramRegions, free_unshared_program, new_compiler_host, new_program,
};
use tsrs_core::{CompilerOptions, P, Tristate, tspath};
use tsrs_tsoptions::new_parsed_command_line;
use tsrs_vfs::vfstest;

#[test]
fn deferred_binding_uses_program_regions() {
    for single_threaded in [Tristate::True, Tristate::False] {
        let regions = Arc::new(ProgramRegions::default());
        let config = new_parsed_command_line(
            P::new(CompilerOptions {
                no_lib: Tristate::True,
                types: Some(Vec::new()),
                ..Default::default()
            }),
            vec!["/index.ts".into()],
            Vec::new(),
            tspath::ComparePathsOptions::default(),
        );
        let host = new_compiler_host(
            "/",
            Arc::new(vfstest::from_map([("/index.ts", "export const value = 1;")], true)),
            "",
            None,
            None,
            None,
        );
        let mut options = ProgramOptions::new(P::new(config), host);
        options.single_threaded = single_threaded;
        options.regions = Some(Arc::clone(&regions));
        let program = new_program(options);
        let file = program.get_source_file("/index.ts").unwrap();
        assert!(!file.is_bound());
        program.bind_source_files();
        let locals = file.locals().unwrap().addr();
        assert!(regions.0.lock().unwrap().values().any(|region| {
            region.chunks().iter().any(|&(start, size)| (start..start + size).contains(&locals))
        }));
        // SAFETY: this program has no shared versions or live checker handles; regions remain owned here.
        unsafe { free_unshared_program(program) };
    }
}
