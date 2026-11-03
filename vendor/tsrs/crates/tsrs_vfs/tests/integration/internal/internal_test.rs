use tsrs_vfs::internal::internal::valid_path;

#[test]
fn valid_path_matches_element_split() {
    // Every name of up to 9 bytes over '/', '.', 'a', against fs.ValidPath's element loop.
    fn by_elements(name: &str) -> bool {
        name == "." || name.split('/').all(|elem| !elem.is_empty() && elem != "." && elem != "..")
    }
    let alphabet = *b"/.a";
    let mut bytes = Vec::new();
    for len in 0..=9 {
        for mut code in 0..3usize.pow(len) {
            bytes.clear();
            bytes.extend(
                std::iter::repeat_with(|| {
                    let byte = alphabet[code % 3];
                    code /= 3;
                    byte
                })
                .take(len as usize),
            );
            let name = std::str::from_utf8(&bytes).unwrap();
            assert_eq!(valid_path(name), by_elements(name), "{name}");
        }
    }
}
