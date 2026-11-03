use tsrs_core::goslices::*;

#[test]
fn stable_sort_matches_std_and_keeps_equal_order() {
    let mut seed = 12345u64;
    for n in [0usize, 1, 2, 19, 20, 21, 39, 40, 41, 100, 257, 1000] {
        let mut v: Vec<(u32, usize)> = (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((seed >> 59) as u32, i)
            })
            .collect();
        let mut expected = v.clone();
        expected.sort_by_key(|e| e.0);
        sort_stable_func(&mut v, |a, b| a.0.cmp(&b.0));
        assert_eq!(v, expected, "n = {n}");
    }
}

// Expected hashes of the resulting index order, printed by the same loop over Go 1.27's slices.SortFunc.
#[test]
fn sort_func_matches_go_order_of_equal_elements() {
    let mut seed = 12345u64;
    let expected: [(usize, u64); 10] = [
        (0, 1_469_598_103_934_665_603),
        (1, 4_953_163_356_653_287_321),
        (2, 11_126_444_148_914_698_056),
        (12, 18_432_498_448_998_557_195),
        (13, 13_263_490_368_736_464_853),
        (50, 10_544_482_255_686_026_482),
        (51, 8_859_204_245_189_759_248),
        (100, 6_651_969_891_758_753_999),
        (257, 17_806_366_977_940_897_185),
        (1000, 5_386_659_266_956_183_601),
    ];
    for (n, want) in expected {
        let mut v: Vec<(i32, usize)> = (0..n)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((seed >> 61) as i32, i)
            })
            .collect();
        sort_func(&mut v, |a, b| a.0.cmp(&b.0));
        assert!(v.windows(2).all(|w| w[0].0 <= w[1].0));
        let mut h = 1_469_598_103_934_665_603_u64;
        h = v.iter().fold(h, |mut h, e| {
            h = (h ^ e.1 as u64).wrapping_mul(1_099_511_628_211);

            h
        });
        assert_eq!(h, want, "n = {n}");
    }
}

#[test]
fn binary_search_finds_insertion_point() {
    let v = [1, 3, 5, 7];
    assert_eq!(binary_search_func(&v, &5, Ord::cmp), (2, true));
    assert_eq!(binary_search_func(&v, &4, Ord::cmp), (2, false));
    assert_eq!(binary_search_func(&v, &9, Ord::cmp), (4, false));
    assert_eq!(binary_search_func(&[] as &[i32], &1, Ord::cmp), (0, false));
}
