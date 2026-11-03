use tsrs_core::json::plain::{is_plain, plain_ascii_len};

/// Agrees with the byte-at-a-time definition for every byte value at every offset of an otherwise plain run.
#[test]
fn matches_bytewise_scan() {
    for len in 0..20 {
        for at in 0..=len {
            for c in 0..=255u8 {
                let mut b = vec![b'a'; len];
                if at < len {
                    b[at] = c;
                }
                // A second special byte after the first must not hide it.
                if at + 1 < len {
                    b[at + 1] = b'"';
                }
                let want = b.iter().position(|&c| !is_plain(c)).unwrap_or(b.len());
                assert_eq!(plain_ascii_len(&b), want, "{b:?}");
            }
        }
    }
}
