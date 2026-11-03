use tsrs_ast::*;
use tsrs_core::undefined_text_range;

use tsrs_core::TextRange;

#[test]
fn deep_clone_reparse_copies_subtree_and_sets_parents() {
    let f = NodeFactory::default();
    let left = f.new_identifier("a");
    left.set_loc(TextRange::new(0, 1));
    let right = f.new_identifier("b");
    right.set_loc(TextRange::new(2, 3));
    let name = f.new_qualified_name(left, right);
    name.set_loc(TextRange::new(0, 3));

    let clone = f.deep_clone_reparse(Some(name)).unwrap();
    assert_ne!(clone, name);
    assert_eq!(clone.loc(), name.loc());
    assert!(clone.flags().intersects(NodeFlags::Reparsed));
    let q = clone.as_qualified_name();
    assert!(q.left != left && q.right != right);
    assert_eq!(q.left.text(), "a");
    assert_eq!(q.left.parent(), Some(clone));

    let synthetic = f.deep_clone_node(Some(name)).unwrap();
    assert_eq!(synthetic.loc(), undefined_text_range());
    assert_eq!(synthetic.as_qualified_name().right.loc(), undefined_text_range());
}

#[test]
fn reparse_cloner_clones_repeatedly() {
    let f = NodeFactory::default();
    let mut cloner = f.reparse_cloner();
    let a = f.new_identifier("a");
    let b = f.new_identifier("b");
    let ca = cloner.clone_node(Some(a)).unwrap();
    let cb = cloner.clone_node(Some(b)).unwrap();
    assert!(ca != a && cb != b && ca != cb);
    assert_eq!(ca.text(), "a");
    assert_eq!(cb.text(), "b");
    assert!(cb.flags().intersects(NodeFlags::Reparsed));
    assert!(cloner.clone_modifiers(None).is_none());
}
