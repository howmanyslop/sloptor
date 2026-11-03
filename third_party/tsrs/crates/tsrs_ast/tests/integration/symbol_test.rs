use tsrs_ast::checkflags::CheckFlags;
use tsrs_ast::symbolflags::SymbolFlags;
use tsrs_ast::*;
use tsrs_core::P;

#[test]
fn optional_symbol_tables_allocate_only_for_values() {
    let source = Symbol::new(SymbolFlags::Class, "source");
    let alias = Symbol::new_shared_data(source);
    let parent = Symbol::new(SymbolFlags::Module, "parent");
    source.set_parent(Some(parent));

    alias.set_members(None);
    source.set_exports(None);
    alias.set_export_symbol(None);
    assert!(source.tables().is_none());
    assert!(alias.tables().is_none());
    assert_eq!(alias.parent(), Some(parent));

    let members = SymbolTable::new();
    alias.set_members(Some(members));
    let tail = source.tables().unwrap();
    assert_eq!(alias.tables(), Some(tail));
    assert_eq!(source.members(), Some(members));
    assert_eq!(source.parent(), Some(parent));

    let exports = SymbolTable::new();
    let export_symbol = Symbol::new(SymbolFlags::Alias, "export");
    source.set_exports(Some(exports));
    alias.set_export_symbol(Some(export_symbol));
    assert_eq!(source.tables(), Some(tail));
    assert_eq!(alias.exports(), Some(exports));
    assert_eq!(source.export_symbol(), Some(export_symbol));

    source.set_members(None);
    alias.set_exports(None);
    source.set_export_symbol(None);
    assert_eq!(source.tables(), Some(tail));
    assert_eq!(alias.members(), None);
    assert_eq!(source.exports(), None);
    assert_eq!(alias.export_symbol(), None);
}

#[test]
fn instantiated_symbol_shares_data_but_not_identity_or_flags() {
    let factory = NodeFactory::default();
    let first = factory.new_identifier("first");
    let second = factory.new_identifier("second");
    let source = Symbol::new(SymbolFlags::Property, "original");
    let parent = Symbol::new(SymbolFlags::Class, "parent");
    let export = Symbol::new(SymbolFlags::Alias, "export");
    source.set_declarations(&[first]);
    source.set_value_declaration(Some(first));
    source.set_parent(Some(parent));
    source.set_export_symbol(Some(export));

    let instance = Symbol::new_shared_data(source);
    let nested = Symbol::new_shared_data(instance);
    assert_eq!(instance.parent_or_tables.get(), source.key());
    assert_eq!(nested.parent_or_tables.get(), source.key());
    assert_ne!(source, instance);
    assert_ne!(instance, nested);
    assert_ne!(get_symbol_id(source), get_symbol_id(instance));
    assert_ne!(get_symbol_id(instance), get_symbol_id(nested));
    assert_eq!(instance.flags(), SymbolFlags::Property | SymbolFlags::Transient);
    assert_eq!(instance.check_flags(), CheckFlags::None);
    assert_eq!(instance.name(), "original");
    assert_eq!(instance.declarations(), &[first]);
    assert_eq!(instance.value_declaration(), Some(first));
    assert_eq!(instance.parent(), Some(parent));
    assert_eq!(instance.export_symbol(), Some(export));

    // Lazy table creation through either symbol changes the same data.
    assert_eq!(get_members(instance), get_members(source));
    assert_eq!(get_exports(source), get_exports(instance));
    let members = SymbolTable::new();
    let exports = SymbolTable::new();
    instance.set_members(Some(members));
    source.set_exports(Some(exports));
    assert_eq!(source.members(), Some(members));
    assert_eq!(instance.exports(), Some(exports));

    source.set_name("renamed");
    instance.set_declarations(&[second]);
    assert_eq!(instance.name(), "renamed");
    assert_eq!(nested.name(), "renamed");
    assert_eq!(source.declarations(), &[second]);
    // Replacing the first declaration preserves the old value declaration in the tail.
    assert_eq!(instance.value_declaration(), Some(first));
    instance.set_value_declaration(Some(second));
    assert_eq!(source.value_declaration(), Some(second));
    instance.set_parent(None);
    source.set_export_symbol(None);
    assert_eq!(source.parent(), None);
    assert_eq!(instance.export_symbol(), None);

    source.set_flags(SymbolFlags::Class);
    source.set_check_flags(CheckFlags::Readonly);
    instance.set_check_flags(CheckFlags::None);
    assert_eq!(instance.name(), "renamed");
    assert_eq!(instance.declarations(), &[second]);
    instance.set_check_flags(CheckFlags::Instantiated | CheckFlags::Late);
    assert_eq!(source.flags(), SymbolFlags::Class);
    assert_eq!(instance.flags(), SymbolFlags::Property | SymbolFlags::Transient);
    assert_eq!(source.check_flags(), CheckFlags::Readonly);
    assert_eq!(instance.check_flags(), CheckFlags::Instantiated | CheckFlags::Late);

    // Neither kind of semantic flags determines the allocation layout.
    instance.set_flags(SymbolFlags::None);
    nested.set_check_flags(CheckFlags::all());
    nested.set_check_flags(CheckFlags::None);
    source.set_check_flags(CheckFlags::all());
    assert_eq!(nested.check_flags().bits(), 0);
    assert_eq!(source.check_flags(), CheckFlags::all());
    assert_eq!(nested.name(), "renamed");
    assert_eq!(instance.declarations(), &[second]);
    assert!(source.tables().is_some());
    assert!(!source.shares_data());
    assert!(instance.shares_data());
}

#[test]
fn symbol_check_flags_reject_layout_bit_without_changing_data() {
    for symbol in [
        Symbol::new(SymbolFlags::None, ""),
        Symbol::new_shared_data(Symbol::new(SymbolFlags::Property, "source")),
    ] {
        symbol.set_check_flags(CheckFlags::Readonly);
        let name = symbol.name();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            symbol.set_check_flags(CheckFlags::from_bits_retain(SHARED_DATA));
        }));
        assert!(result.is_err());
        assert_eq!(symbol.check_flags(), CheckFlags::Readonly);
        assert_eq!(symbol.name(), name);
    }
}

#[test]
fn shared_symbol_keeps_long_declarations_and_value_declaration() {
    let factory = NodeFactory::default();
    let first = factory.new_identifier("first");
    let second = factory.new_identifier("second");
    let source = Symbol::new(SymbolFlags::Property, "long");
    let declarations = tsrs_core::alloc_vec(vec![first; 65536]);
    source.set_declarations_static(declarations);
    source.set_value_declaration(Some(first));
    let instance = Symbol::new_shared_data(source);
    assert_eq!(instance.declarations().as_ptr(), declarations.as_ptr());
    assert_eq!(instance.declarations().len(), declarations.len());
    #[cfg(target_pointer_width = "64")]
    assert!(source.own_data().declarations.get().is_long());
    instance.append_declarations(&[second]);
    assert_eq!(source.declarations().len(), 65537);
    assert_eq!(source.declarations()[65536], second);
    instance.set_declarations_static(&[]);
    assert_eq!(source.declarations(), &[]);
    assert_eq!(instance.value_declaration(), Some(first));
    instance.set_value_declaration(None);
    assert_eq!(source.value_declaration(), None);
}

#[test]
fn name_filter_keeps_every_key() {
    // Linear and indexed tables, keys up to and past 16 bytes, and an odd key (not its symbol's name).
    let small = SymbolTable::new();
    let large = SymbolTable::new();
    let mut names = Vec::new();
    for i in 0..200 {
        let name: &'static str =
            Box::leak(format!("member{i}{}", "x".repeat(i % 23)).into_boxed_str());
        names.push(name);
        let table = if i < 10 { &small } else { &large };
        table.set(name, Symbol::new(SymbolFlags::Property, name));
    }
    small.set("odd", Symbol::new(SymbolFlags::Property, "other"));
    names.push("odd");
    let mut filter = NameFilter::default();
    filter.add_keys(&small);
    filter.add_keys(&large);
    for name in names {
        assert!(filter.may_contain(HashedName::new(name)), "{name}");
    }
    assert!(!NameFilter::default().may_contain(HashedName::new("member0")));
}

#[test]
fn index_finds_every_key_through_growth_and_deletes() {
    // Past the linear limit and 255 entries, presized and grown, with deletes, an odd key and a replaced symbol.
    for presized in [0, 20, 300] {
        let table =
            if presized == 0 { SymbolTable::new() } else { SymbolTable::with_capacity(presized) };
        let names: Vec<&'static str> =
            (0..600).map(|i| &*Box::leak(format!("k{i}").into_boxed_str())).collect();
        let mut expected: Vec<(&'static str, P<Symbol>)> = Vec::new();
        for (i, &name) in names.iter().enumerate() {
            let symbol = Symbol::new(SymbolFlags::Property, if i == 7 { "odd" } else { name });
            table.set(name, symbol);
            expected.push((name, symbol));
            if i % 97 == 50 {
                let gone = expected.remove(i / 3).0;
                table.delete(gone);
                assert!(table.lookup(gone).is_none(), "{gone}");
            }
            if i % 89 == 3 {
                let replaced = Symbol::new(SymbolFlags::Property, expected[i / 2].0);
                table.set(expected[i / 2].0, replaced);
                expected[i / 2].1 = replaced;
            }
            for &(name, symbol) in &expected {
                assert!(table.lookup(name) == Some(symbol), "{name} after {i}");
            }
            assert!(table.lookup("absent").is_none());
        }
        assert_eq!(table.entries(), expected);
    }
}
