use tsrs_compiler::checkerpool::default_checkers_for_parallelism;

/// The default is every core up to 16, half the cores above that, never below Go's 4 or above 32: a 17-33 core
/// machine keeps 16 (half its cores rounds below the small-machine count), 34 cores get 17, 64 or more get 32.
#[test]
fn every_core_up_to_sixteen_then_half_the_cores() {
    let table = [
        (1, 4),
        (2, 4),
        (4, 4),
        (6, 6),
        (8, 8),
        (9, 9),
        (12, 12),
        (16, 16),
        (18, 16),
        (32, 16),
        (33, 16),
        (34, 17),
        (48, 24),
        (64, 32),
        (128, 32),
    ];
    for (cores, checkers) in table {
        assert_eq!(default_checkers_for_parallelism(cores), checkers, "{cores} cores");
    }
}
