use std::sync::Arc;

use tsrs_core::arena::Region;
use tsrs_core::tspath::Path;
use tsrs_core::{CompilerOptions, P};
use tsrs_module::symlinks::KnownSymlinks;
use tsrs_vfs::{FS, vfstest};

use tsrs_compiler::projectreferencedtsfakinghost::ProjectReferenceDtsFakingVfs;
use tsrs_compiler::projectreferencefilemapper::{ProjectReferenceFileMapper, resolution_host_for};

// A symlinked node_modules directory the build never probed, first seen while a region is the allocation target
// (a checker's, looking up a package.json for a module specifier): the host shared by later program versions must
// not keep its records in that region.
#[test]
fn symlinked_directory_first_seen_in_a_region_is_not_kept_there() {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(
        [
            ("/lib/src/deep/thing.ts", vfstest::MapFile::from("export {};")),
            ("/app/node_modules/@x/lib", vfstest::symlink("/lib")),
        ],
        true,
    ));
    let host = resolution_host_for(tsrs_compiler::new_compiler_host("/", fs, "", None, None, None));
    let config = P::new(tsrs_tsoptions::new_parsed_command_line(
        P::new(CompilerOptions::default()),
        Vec::new(),
        Vec::new(),
        tsrs_core::tspath::ComparePathsOptions::default(),
    ));
    let mapper: &'static ProjectReferenceFileMapper =
        Box::leak(Box::new(ProjectReferenceFileMapper::new(config, true)));
    let vfs = ProjectReferenceDtsFakingVfs {
        host,
        project_reference_file_mapper: mapper,
        known_symlinks: KnownSymlinks::default(),
    };
    let region = Region::new(1 << 20);
    {
        let _scope = region.enter();
        assert!(vfs.directory_exists("/app/node_modules/@x/lib/src/deep"));
    }
    let in_region = |addr: usize| Region::containing(addr).is_some_and(|r| r.ptr_eq(&region));
    let link = vfs
        .known_symlinks
        .directories()
        .load(&Path::from("/app/node_modules/@x/lib/src/deep/"))
        .flatten()
        .expect("the link is recorded");
    assert_eq!(link.real_path.as_str(), "/lib/src/deep/");
    assert!(!in_region(link.addr()), "directory link allocated in the region");
    let set = vfs
        .known_symlinks
        .directories_by_realpath()
        .load(&link.real_path)
        .expect("the realpath's symlink set is recorded");
    assert!(!in_region(set.addr()), "symlink set allocated in the region");
}
