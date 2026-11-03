use tsrs_core::collections::SyncSet;

use rustc_hash::FxHashMap;
use tsrs_core::bfs::*;

fn graph(edges: &[(&'static str, &[&'static str])]) -> FxHashMap<&'static str, Vec<&'static str>> {
    edges.iter().map(|(k, v)| (*k, v.to_vec())).collect()
}

#[test]
fn test_breadth_first_search_parallel() {
    let g = graph(&[("A", &["B", "C"]), ("B", &["D"]), ("C", &["D"]), ("D", &[])]);
    let children = |n: &&'static str| g[n].clone();

    let result = breadth_first_search_parallel("A", children, |node| (*node == "D", true));
    assert!(result.stopped);
    assert_eq!(result.path, vec!["D", "B", "A"]);

    let mut visited_nodes = Vec::new();
    let result = breadth_first_search_parallel("A", children, |node| {
        visited_nodes.push(*node);
        (false, false)
    });
    assert!(!result.stopped);
    assert_eq!(result.path, [] as [&str; 0]);
    visited_nodes.sort_unstable();
    assert_eq!(visited_nodes, vec!["A", "B", "C", "D"]);

    // early termination
    let g2 = graph(&[
        ("Root", &["L1A", "L1B"]),
        ("L1A", &["L2A", "L2B"]),
        ("L1B", &["L2C"]),
        ("L2A", &["L3A"]),
        ("L2B", &[]),
        ("L2C", &[]),
        ("L3A", &[]),
    ]);
    let visited = SyncSet::default();
    breadth_first_search_parallel_ex(
        "Root",
        |n: &&'static str| g2[n].clone(),
        |node| (*node == "L2B", true),
        BreadthFirstSearchOptions { visited: Some(&visited), preprocess_level: None },
        |n| *n,
    );
    assert!(visited.has(&"Root"));
    assert!(visited.has(&"L1A"));
    assert!(visited.has(&"L2B"));
    assert!(!visited.has(&"L3A"));

    assert!(visited.has(&"L1B"));
    assert!(visited.has(&"L2A"));

    // returns fallback when no other result found
    let visited = SyncSet::default();
    let result = breadth_first_search_parallel_ex(
        "A",
        children,
        |node| (*node == "A", false),
        BreadthFirstSearchOptions { visited: Some(&visited), preprocess_level: None },
        |n| *n,
    );
    assert!(!result.stopped);
    assert_eq!(result.path, vec!["A"]);
    assert!(visited.has(&"B") && visited.has(&"C") && visited.has(&"D"));

    // returns a stop result over a fallback
    let result = breadth_first_search_parallel("A", children, |node| match *node {
        "A" => (true, false),
        "D" => (true, true),
        _ => (false, false),
    });
    assert!(result.stopped);
    assert_eq!(result.path, vec!["D", "B", "A"]);
}
