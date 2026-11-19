use tsrs_ast::{Kind, SourceFileParseOptions};
use tsrs_core::{ScriptKind, arena::Region};

#[test]
fn lazy_jsdoc_survives_parser_reuse_and_request_region_release() {
    // Registered regions enable process-wide owner routing. Keep this case in its own process.
    const NAME: &str = "jsdoc_regions::lazy_jsdoc_survives_parser_reuse_and_request_region_release";
    if std::env::var_os("TSRS_ISOLATED_TEST").is_none_or(|name| name != NAME) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env("TSRS_ISOLATED_TEST", NAME)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
        assert!(stdout.contains("1 passed;"), "isolated case did not run: {stdout}");
        return;
    }

    let owner = Region::new(4096);
    let file = {
        let _scope = owner.enter();
        tsrs_parser::parse_source_file(
            SourceFileParseOptions { file_name: "/a.ts".to_string(), ..Default::default() },
            "/** @param {number} value */\nfunction a(value: number) {}\n\
             /** @param {string} text */\nfunction b(text: string) {}\nfunction empty() {}",
            ScriptKind::TS,
        )
    };
    let statements = file.statements().nodes();
    let first = {
        let request = Region::new(4096);
        let _scope = request.enter();
        let docs = statements[0].jsdoc(Some(file.get()));
        assert_eq!(docs.len(), 1);
        assert!(Region::containing(docs.as_ptr().addr()).unwrap().ptr_eq(&owner));
        docs
    };
    {
        let request = Region::new(4096);
        let _scope = request.enter();
        let second = statements[1].jsdoc(Some(file.get()));
        assert_eq!(second.len(), 1);
        assert!(Region::containing(second.as_ptr().addr()).unwrap().ptr_eq(&owner));
        assert_ne!(first[0], second[0]);
        assert_eq!(second[0].parent(), Some(statements[1]));
        assert_eq!(statements[2].jsdoc(Some(file.get())), []);
    }
    assert_eq!(first[0].parent(), Some(statements[0]));
    assert_eq!(first[0].as_jsdoc().tags().unwrap().nodes()[0].kind(), Kind::JSDocParameterTag);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let cached = statements[0].jsdoc(Some(file.get()));
                assert!(std::ptr::eq(first, cached));
            });
        }
    });
}
