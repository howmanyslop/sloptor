use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use tsrs_contentmapper::process::{find_executable, path_ext};

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "tsrs-mapper-path-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn test_windows_path_ext() {
    assert_eq!(path_ext(""), [".com", ".exe", ".bat", ".cmd"]);
    assert_eq!(path_ext(";.EXE;;CuStOm;.CMD;"), [".exe", ".custom", ".cmd"]);
    assert_eq!(path_ext(";;"), [] as [String; 0]);
}

#[test]
fn test_windows_find_executable() {
    let temp = TempDir::new();
    let dir = temp.0.join("runtime.with.dots and spaces");
    std::fs::create_dir(&dir).unwrap();
    for name in ["node.exe", "node.custom", "node", "foo.bat.exe", ".hidden", "plain"] {
        std::fs::write(dir.join(name), "").unwrap();
    }
    // Directories are never executable candidates, even if their names have a preferred extension.
    std::fs::create_dir(dir.join("node.com")).unwrap();

    let defaults = path_ext("");
    assert_eq!(find_executable(&dir.join("node"), &defaults), Some(dir.join("node.exe")));
    assert_eq!(
        find_executable(&dir.join("node"), &path_ext("CUSTOM;.EXE")),
        Some(dir.join("node.custom"))
    );
    assert_eq!(
        find_executable(&dir.join("node.exe"), &path_ext(".custom")),
        Some(dir.join("node.exe"))
    );
    assert_eq!(find_executable(&dir.join("foo.bat"), &defaults), Some(dir.join("foo.bat.exe")));
    assert_eq!(find_executable(&dir.join(".hidden"), &defaults), Some(dir.join(".hidden")));
    assert_eq!(find_executable(&dir.join("plain"), &defaults), None);
    assert_eq!(find_executable(&dir.join("node"), &path_ext(";;")), Some(dir.join("node")));
    assert_eq!(find_executable(&dir.join("node.com"), &path_ext(";;")), None);
    assert_eq!(find_executable(&dir.join("missing"), &defaults), None);
}
