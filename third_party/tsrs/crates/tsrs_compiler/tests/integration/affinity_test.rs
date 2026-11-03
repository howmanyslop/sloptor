use tsrs_compiler::affinity::ModuleAffinity;

// Modules 0 and 1 (worth 3 and 2); groups 0 and 2 have module 0, group 1 has module 1; each group weighs 1.
fn affinity() -> ModuleAffinity {
    ModuleAffinity {
        worth: vec![3.0, 2.0],
        starts: vec![0, 1, 2, 3],
        modules: vec![0, 1, 0],
        total: vec![3.0, 2.0, 3.0],
        scale: vec![1.0, 1.0, 1.0],
        words: 1,
        holders: vec![0; 2],
        affinity: vec![0.0; 2],
    }
}

/// A group moves to the checker that already holds its modules when that checker has room under the cap (two
/// checkers, three groups of weight 1: at most 2 per checker); a move that would bring a module to a checker
/// without it, or go over the cap, is not made.
#[test]
fn refine_moves_a_group_to_its_modules_within_the_cap() {
    let mut placed = vec![0, 0, 1];
    affinity().refine(&mut placed, &[1, 1, 1], 2);
    assert_eq!(placed, vec![1, 0, 1]);
    // Already the best placement under the cap: nothing moves.
    let mut placed = vec![1, 0, 1];
    affinity().refine(&mut placed, &[1, 1, 1], 2);
    assert_eq!(placed, vec![1, 0, 1]);
    // Group 2 cannot join group 0 on checker 0, which is full; group 0 cannot move to checker 1, which is full.
    let mut placed = vec![0, 1, 1];
    affinity().refine(&mut placed, &[1, 1, 2], 2);
    assert_eq!(placed, vec![0, 1, 1]);
}
