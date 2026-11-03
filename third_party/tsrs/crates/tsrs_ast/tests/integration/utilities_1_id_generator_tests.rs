use tsrs_ast::*;

#[test]
fn block_ids_decode_without_colliding_with_ordinary_ids() {
    let factory = NodeFactory::default();
    let node = factory.new_token(Kind::EndOfFile);
    let other_node = factory.new_token(Kind::EndOfFile);
    let node_generator = NodeIdGenerator::default();
    let block_node_id = node_generator.get_node_id(node);
    let ordinary_node_id = get_node_id(other_node);
    assert!(block_node_id.0 >= BLOCK_ID_OFFSET);
    assert!(ordinary_node_id.0 < BLOCK_ID_OFFSET);
    assert_eq!(node_generator.get_node_id(node), block_node_id);
    assert_eq!(node_generator.get_node_id(other_node), ordinary_node_id);
    assert_eq!(get_assigned_node_id(node), Some(block_node_id));

    let symbol = Symbol::new(SymbolFlags::None, "");
    let other_symbol = Symbol::new(SymbolFlags::None, "");
    let symbol_generator = SymbolIdGenerator::default();
    let block_symbol_id = symbol_generator.get_symbol_id(symbol);
    let ordinary_symbol_id = get_symbol_id(other_symbol);
    assert!(block_symbol_id.0 >= BLOCK_ID_OFFSET);
    assert!(ordinary_symbol_id.0 < BLOCK_ID_OFFSET);
    assert_eq!(symbol_generator.get_symbol_id(symbol), block_symbol_id);
    assert_eq!(symbol_generator.get_symbol_id(other_symbol), ordinary_symbol_id);
    assert_eq!(get_assigned_symbol_id(symbol), Some(block_symbol_id));
}
