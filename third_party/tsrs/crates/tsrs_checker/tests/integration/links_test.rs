use rustc_hash::FxHashMap;
use std::cell::Cell;
use tsrs_ast as ast;
use tsrs_ast::Symbol;
use tsrs_ast::SymbolFlags;
use tsrs_checker::*;
use tsrs_core::P;
use tsrs_core::PKey;

#[test]
fn link_store_preserves_nil_key_through_growth() {
    let mut store = LinkStore::<u64, Cell<u32>>::default();
    let nil_links = store.get(None);
    nil_links.set(42);
    for i in 0..4096 {
        let key = P::new(i);
        let links = store.get(key);
        assert_ne!(links, nil_links);
        links.set(i as u32);
    }
    assert_eq!(store.get(None), nil_links);
    assert_eq!(nil_links.get().get(), 42);
}

#[test]
fn type_node_and_signature_links_keep_mixed_ids_separate() {
    let factory = ast::NodeFactory::default();
    let nodes: Vec<_> =
        std::iter::repeat_with(|| factory.new_identifier("node")).take(4096).collect();
    let mut first_types = NodeValueLinkStore::<TypeNodeLinks>::default();
    let mut first_signatures = NodeValueLinkStore::<SignatureLinks>::default();
    let mut second_types = NodeValueLinkStore::<TypeNodeLinks>::default();
    let mut second_signatures = NodeValueLinkStore::<SignatureLinks>::default();
    for (i, &node) in nodes.iter().enumerate() {
        if i % 2 == 0 {
            ast::get_node_id(node);
        }
        assert!(!first_types.has(node), "type links already present at {i}");
        assert!(!first_signatures.has(node), "signature links already present at {i}");
        for (types, signatures, id) in [
            (&mut first_types, &mut first_signatures, i + 1),
            (&mut second_types, &mut second_signatures, i + 2),
        ] {
            let t = Type::alloc(
                TypeFlags::Any,
                ObjectFlags::None,
                TypeId(id as u32),
                IntrinsicType::default(),
            );
            let s = P::new(Signature::default());
            s.id.set(SignatureId(id as u32));
            types.get(node).resolved_type.set(Some(t));
            signatures.get(node).resolved_signature.set(Some(s));
        }
    }
    for (i, &node) in nodes.iter().enumerate() {
        assert_eq!(first_types.get(node).resolved_type.get().unwrap().id, TypeId(i as u32 + 1));
        assert_eq!(
            first_signatures.get(node).resolved_signature.get().unwrap().id.get(),
            SignatureId(i as u32 + 1)
        );
        assert_eq!(second_types.get(node).resolved_type.get().unwrap().id, TypeId(i as u32 + 2));
        assert_eq!(
            second_signatures.get(node).resolved_signature.get().unwrap().id.get(),
            SignatureId(i as u32 + 2)
        );
    }
    let clone = nodes[0].clone_node(&factory);
    assert!(!first_types.has(clone));
    assert!(!first_signatures.has(clone));
}

#[test]
fn node_value_link_fork_copies_a_frozen_record_on_first_write() {
    if !tsrs_core::sharedgraph::COMPILED_IN {
        return;
    }
    let node = ast::NodeFactory::default().new_identifier("node");
    let seed = Box::leak(Box::new(NodeValueLinkStore::<Cell<u32>>::default()));
    seed.get(node).set(7);
    let seed: &'static NodeValueLinkStore<Cell<u32>> = seed;
    let mut fork = seed.fork();
    assert_eq!(fork.try_get(node).unwrap().get().get(), 7);
    fork.get(node).set(8);
    assert_eq!(fork.get(node).get().get(), 8);
    assert_eq!(seed.try_get(node).unwrap().get().get(), 7);
}

#[test]
fn symbol_link_store_separates_reserved_and_ordinary_ids() {
    assert_eq!(dense_link_key(ast::BLOCK_ID_OFFSET), Some(0));
    assert_eq!(
        dense_link_key(ast::BLOCK_ID_OFFSET + MAX_PAGE_LINK_COUNT - 1),
        Some(MAX_PAGE_LINK_COUNT - 1)
    );
    assert_eq!(dense_link_key(ast::BLOCK_ID_OFFSET + MAX_PAGE_LINK_COUNT), None);
    let ordinary = Symbol::new(SymbolFlags::None, "");
    let ordinary_id = ast::get_symbol_id(ordinary).0;
    assert!(ordinary_id < ast::BLOCK_ID_OFFSET);

    let mut store: SymbolArenaLinkStore<Cell<u32>> = SymbolArenaLinkStore::default();
    store.get(ordinary).set(7);
    let symbols: Vec<_> =
        std::iter::repeat_with(|| Symbol::new(SymbolFlags::None, "")).take(600).collect();
    for (index, &symbol) in symbols.iter().enumerate() {
        assert!(store.try_get_if_id_assigned(symbol).is_none());
        let link = store.get(symbol);
        link.set(index as u32 + 100);
        assert_eq!(store.get(symbol), link, "symbol {index} changed links immediately");
        assert!(ast::get_symbol_id(symbol).0 >= ast::BLOCK_ID_OFFSET);
    }
    assert_eq!(store.get(ordinary).get().get(), 7);
    for (index, &symbol) in symbols.iter().enumerate() {
        assert_eq!(store.get(symbol).get().get(), index as u32 + 100);
        assert_eq!(store.try_get_if_id_assigned(symbol).unwrap().get().get(), index as u32 + 100);
    }
}

// A narrow group stores slot offsets from its first slot in 16 bits. A group that gets a link again after more
// than 2^16 other links must get its dense form and keep every earlier id's slot.
#[test]
fn narrow_groups_turn_dense_without_losing_slots() {
    let mut store: IdLinkStore<Cell<u64>> = IdLinkStore::default();
    let mut ids: Vec<u64> = vec![5, 7];
    ids.extend((0..70_000u64).map(|i| 128 + i));
    ids.extend([9, 127]);
    for &id in &ids {
        store.get(id).set(id + 1);
    }
    assert!(store.index[0].unwrap().dense.get().is_some());
    assert!(store.index[1].unwrap().dense.get().is_none());
    for &id in &ids {
        assert_eq!(store.try_get(id).map(|v| (*v).get()), Some(id + 1), "id {id}");
    }
    for id in [0u64, 6, 8, 10, 126, 70_200] {
        assert!(store.try_get(id).is_none(), "id {id}");
    }
}

fn scrambled(mut ids: Vec<u64>, seed: u64) -> Vec<u64> {
    let mut x = seed;
    for i in (1..ids.len()).rev() {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ids.swap(i, (x >> 33) as usize % (i + 1));
    }
    ids
}

// Inline values: an id must reach its own cell through both levels wherever it falls in its group and block, and a
// cell must stay where it is while later ids allocate tables and groups and grow the block index (callers keep
// the reference across other accesses). An id outside every allocated group reads as absent.
#[test]
fn inline_ids_reach_their_own_stable_cells() {
    let mut store: InlineIdStore<Cell<u32>> = InlineIdStore::default();
    let mut ids: Vec<u64> = (0..4000u64).map(|i| (i * 7919) % 300_000).collect();
    ids.extend([
        0,
        1,
        31,
        32,
        33,
        1023,
        1024,
        1025,
        65_535,
        65_536,
        65_537,
        (1 << 20) - 1,
        1 << 20,
        (1 << 24) + 5,
    ]);
    let ids = scrambled(ids, 12345);
    let mut expected: FxHashMap<u64, u32> = FxHashMap::default();
    let mut held: Vec<(u64, *const Cell<u32>)> = Vec::new();
    for (n, &id) in ids.iter().enumerate() {
        let value = store.get(id);
        if value.get() == 0 {
            value.set(n as u32 + 1);
            expected.insert(id, n as u32 + 1);
        }
        held.push((id, std::ptr::from_ref(value)));
    }
    for &(id, address) in &held {
        assert!(std::ptr::eq(store.get(id), address), "id {id} moved");
    }
    for id in (0..300_000u64).chain([(1 << 20) - 1, 1 << 20, (1 << 24) + 5]) {
        assert_eq!(
            store.try_get(id).map_or(0, std::cell::Cell::get),
            expected.get(&id).copied().unwrap_or(0),
            "id {id}"
        );
    }
    assert!(store.try_get(5_000_000).is_none());
    assert!(store.try_get((1 << 24) + 64).is_none()); // same block as (1 << 24) + 5, another group
    assert!(store.try_get(1 << 40).is_none());
    store.get(1 << 40).set(7);
    assert_eq!(store.try_get(1 << 40).map(std::cell::Cell::get), Some(7));
}

// The block index grows by a quarter, once per new top block: values written before every growth must still be
// found after it, through the new index.
#[test]
fn inline_index_growth_keeps_every_value() {
    let mut store: InlineIdStore<Cell<u32>> = InlineIdStore::default();
    for b in 0..3000u64 {
        store.get(b * 1024 + b % 1024).set(b as u32 + 1);
    }
    for b in 0..3000u64 {
        assert_eq!(
            store.try_get(b * 1024 + b % 1024).map(std::cell::Cell::get),
            Some(b as u32 + 1),
            "block {b}"
        );
        assert_eq!(
            store.try_get(b * 1024 + (b + 32) % 1024).map(std::cell::Cell::get),
            None,
            "block {b}"
        );
    }
}

#[derive(Default)]
struct Keyed {
    n: Cell<u32>,
    key: Cell<PKey>,
}

tsrs_checker::bitwise_link_copy!(Keyed);

impl KeyedLinks for Keyed {
    fn link_key(&self) -> &Cell<PKey> {
        &self.key
    }
}

// The table holds only indexes and finds a key in the value it points at: through ~15 rehashes (60,000 keys
// from an empty table, 59 chunks of values), every key must keep reaching its own value at its first address, a
// key never inserted must miss (about one probe in 128 meets a matching 7-bit tag and must be rejected by the key
// in the value), and the value must hold its key.
#[test]
fn keyed_store_finds_each_key_through_rehashes() {
    let keys: Vec<P<u64>> = (0..60_000u64).map(P::new).collect();
    let order: Vec<u64> = scrambled((0..keys.len() as u64).collect(), 777);
    let mut store: KeyedLinkStore<u64, Keyed> = KeyedLinkStore::default();
    let mut held: Vec<Option<P<Keyed>>> = vec![None; keys.len()];
    for &i in &order {
        let value = store.get(keys[i as usize]);
        assert_eq!(value.n.get(), 0);
        value.n.set(i as u32 + 1);
        held[i as usize] = Some(value);
    }
    for (i, &key) in keys.iter().enumerate() {
        let value = store.try_get(key).unwrap();
        assert!(Some(value) == held[i], "key {i} moved");
        assert_eq!(store.get(key), value);
        assert_eq!(value.n.get(), i as u32 + 1);
        assert_eq!(value.key.get(), key.key());
    }
    for absent in (0..10_000u64).map(P::new) {
        assert!(store.try_get(absent).is_none());
        assert!(!store.has(absent));
    }
}

// Reference kinds live in the slot: meanings must accumulate per symbol through the table's growth, and a symbol
// never referenced must read as `None` without getting a record.
#[test]
fn reference_kinds_accumulate_in_their_slots() {
    let symbols: Vec<P<Symbol>> =
        std::iter::repeat_with(|| Symbol::new(SymbolFlags::None, "")).take(20000).collect();
    let mut store = SymbolReferenceLinkStore::default();
    for (i, &symbol) in symbols.iter().enumerate().filter(|(i, _)| i % 3 != 0) {
        store.add_reference_kinds(symbol, SymbolFlags::Value);
        if i % 2 == 0 {
            store.add_reference_kinds(symbol, SymbolFlags::Type);
        }
    }
    for (i, &symbol) in symbols.iter().enumerate() {
        let expected = match (i % 3 != 0, i % 2 == 0) {
            (false, _) => SymbolFlags::None,
            (true, false) => SymbolFlags::Value,
            (true, true) => SymbolFlags::Value | SymbolFlags::Type,
        };
        assert_eq!(store.reference_kinds(symbol), expected, "symbol {i}");
    }
    assert_eq!(store.slots.len(), symbols.len() - symbols.len().div_ceil(3));
}

// A wrong index hands out another id's links: the checker would read a foreign symbol's type without any visible
// error. Every id must map to the slot it was given, in a scrambled order across many groups.
#[test]
fn groups_map_ids_to_their_slots() {
    let mut store: IdLinkStore<Cell<u32>> = IdLinkStore::default();
    let mut ids: Vec<u64> = (0..3000u64).map(|i| (i * 7919) % 40_000).collect();
    let mut x = 12345u64;
    for i in (1..ids.len()).rev() {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ids.swap(i, (x >> 33) as usize % (i + 1));
    }
    let mut expected: FxHashMap<u64, u32> = FxHashMap::default();
    for (n, &id) in ids.iter().enumerate() {
        let links = store.get(id);
        if links.get().get() == 0 {
            links.set(n as u32 + 1);
            expected.insert(id, n as u32 + 1);
        }
    }
    for id in 0..40_000u64 {
        assert_eq!(store.try_get(id).map(|v| (*v).get()), expected.get(&id).copied(), "id {id}");
    }
    assert_eq!(store.try_get(1 << 40), None);
    store.get(1 << 40).set(7);
    assert_eq!(store.try_get(1 << 40).map(|v| (*v).get()), Some(7));
}
