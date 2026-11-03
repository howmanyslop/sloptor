use tsrs_ast::generated::NodeDataTag;
use tsrs_ast::*;

use std::rc::Rc;

use tsrs_core::{P, TextRange};

fn set_parents(node: P<Node>) {
    node.for_each_child(&mut |child| {
        child.set_parent(Some(node));
        set_parents(child);
        false
    });
}

fn children(node: P<Node>) -> Vec<P<Node>> {
    node.iter_children()
}

#[test]
fn build_walk_and_cast() {
    let f = NodeFactory::default();
    let a = f.new_identifier("a");
    let plus = f.new_token(Kind::PlusToken);
    let b = f.new_identifier("b");
    let bin = f.new_binary_expression(None, a, None, plus, b);
    let stmt = f.new_expression_statement(bin);
    let list = f.new_node_list(vec![stmt]);
    let block = f.new_block(list, false);
    set_parents(block);

    assert_eq!(f.node_count(), 6);
    assert!(is_binary_expression(bin));
    assert!(!is_identifier(bin));
    assert_eq!(children(bin), vec![a, plus, b]);
    assert_eq!(children(block), vec![stmt]);
    assert_eq!(bin.as_binary_expression().left(), a);
    assert_eq!(bin.as_binary_expression().operator_token.kind(), Kind::PlusToken);
    assert_eq!(stmt.expression(), Some(bin));
    assert_eq!(block.statements(), &[stmt]);
    assert_eq!(a.text(), "a");
    assert_eq!(a.parent(), Some(bin));
    assert_eq!(bin.parent(), Some(stmt));
    assert_eq!(stmt.parent(), Some(block));
    assert!(block.contains(Some(a)));
    assert!(block.locals_container_data().is_some());
    assert!(bin.declaration_data().is_some());
    assert!(a.has_flow_node_data());
    assert!(plus.declaration_data().is_none());
    assert_eq!(a.loc(), TextRange::synthetic());

    // Stopping early propagates true.
    let mut seen = 0;
    assert!(bin.for_each_child(&mut |_| {
        seen += 1;
        seen == 2
    }));
    assert_eq!(seen, 2);
}

#[test]
fn token_factory_uses_canonical_payloads() {
    let f = NodeFactory::default();

    let identifier = f.new_token(Kind::Identifier);
    assert_eq!(identifier.as_identifier().text(), "");
    assert!(identifier.has_flow_node_data());

    let literal = f.new_token(Kind::StringLiteral);
    assert_eq!(literal.as_string_literal().text(), "");
    assert!(literal.literal_like_data().is_some());

    let keyword = f.new_token(Kind::NullKeyword);
    assert!(matches!(keyword.data(), NodeData::KeywordExpression(_)));
    assert!(keyword.has_flow_node_data());
    assert!(matches!(keyword.clone_node(&f).data(), NodeData::KeywordExpression(_)));

    let type_keyword = f.new_token(Kind::NumberKeyword);
    assert!(matches!(type_keyword.data(), NodeData::KeywordTypeNode));
    assert!(type_keyword.subtree_facts().contains(SubtreeFacts::ContainsTypeScript));

    assert!(matches!(f.new_token(Kind::PlusToken).data(), NodeData::Token));
    assert_eq!(f.text_count(), 0);
    assert_eq!(f.node_count(), 6);
}

#[test]
fn factory_kind_parameters_reject_noncanonical_kinds() {
    let f = NodeFactory::default();
    let leaf = f.new_identifier("name");
    let list = f.new_node_list(vec![]);

    for kind in [Kind::ForInStatement, Kind::ForOfStatement] {
        f.new_for_in_or_of_statement(kind, None, leaf, leaf, leaf).as_for_in_or_of_statement();
    }
    for kind in [Kind::CaseClause, Kind::DefaultClause] {
        f.new_case_or_default_clause(kind, None, list).as_case_or_default_clause();
    }
    for kind in [Kind::ArrayBindingPattern, Kind::ObjectBindingPattern] {
        f.new_binding_pattern(kind, list).as_binding_pattern();
    }
    for kind in [
        Kind::FalseKeyword,
        Kind::ImportKeyword,
        Kind::NullKeyword,
        Kind::SuperKeyword,
        Kind::ThisKeyword,
        Kind::TrueKeyword,
    ] {
        f.new_keyword_expression(kind).as_keyword_expression();
    }
    for kind in [
        Kind::VoidKeyword,
        Kind::AnyKeyword,
        Kind::BooleanKeyword,
        Kind::IntrinsicKeyword,
        Kind::NeverKeyword,
        Kind::NumberKeyword,
        Kind::ObjectKeyword,
        Kind::StringKeyword,
        Kind::SymbolKeyword,
        Kind::UndefinedKeyword,
        Kind::UnknownKeyword,
        Kind::BigIntKeyword,
    ] {
        f.new_keyword_type_node(kind).as_keyword_type_node();
    }
    for kind in [Kind::JSDocParameterTag, Kind::JSDocPropertyTag] {
        f.new_jsdoc_parameter_or_property_tag(kind, leaf, leaf, false, None, false, None)
            .as_jsdoc_parameter_or_property_tag();
    }

    let rejects = |make: &dyn Fn()| {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(make)).is_err());
    };
    let before_invalid = f.node_count();
    rejects(&|| {
        f.new_for_in_or_of_statement(Kind::FunctionDeclaration, None, leaf, leaf, leaf);
    });
    rejects(&|| {
        f.new_case_or_default_clause(Kind::FunctionDeclaration, None, list);
    });
    rejects(&|| {
        f.new_binding_pattern(Kind::FunctionDeclaration, list);
    });
    rejects(&|| {
        f.new_keyword_expression(Kind::FunctionDeclaration);
    });
    rejects(&|| {
        f.new_keyword_type_node(Kind::FunctionDeclaration);
    });
    rejects(&|| {
        f.new_jsdoc_parameter_or_property_tag(
            Kind::FunctionDeclaration,
            leaf,
            leaf,
            false,
            None,
            false,
            None,
        );
    });
    rejects(&|| {
        f.new_token(Kind::FunctionDeclaration);
    });
    rejects(&|| {
        f.new_token(Kind::Count);
    });
    assert_eq!(f.node_count(), before_invalid);

    f.new_token(Kind::Unknown).as_token();
    f.new_token(Kind::PlusToken).as_token();
    rejects(&|| {
        f.new_token(Kind::Identifier).as_token();
    });
    rejects(&|| {
        f.new_identifier("other").as_string_literal();
    });

    assert_eq!(std::mem::offset_of!(tsrs_ast::ast::NodeAlloc<Identifier>, node), 0);
    assert_eq!(std::mem::offset_of!(tsrs_ast::ast::NodeAlloc<SourceFile>, node), 0);
    assert_eq!(std::mem::offset_of!(tsrs_ast::ast::NodeAlloc<FlowSwitchClauseData>, node), 0);
    assert_eq!(std::mem::offset_of!(tsrs_ast::ast::NodeAlloc<FlowReduceLabelData>, node), 0);
}

#[test]
#[should_panic(expected = "as_binary_expression called on")]
fn cast_mismatch_panics() {
    let f = NodeFactory::default();
    let a = f.new_identifier("a");
    a.as_binary_expression();
}

#[test]
fn source_file_back_pointer() {
    let f = NodeFactory::default();
    let stmt = f.new_empty_statement();
    let statements = f.new_node_list(vec![stmt]);
    let eof = f.new_token(Kind::EndOfFile);
    let opts = SourceFileParseOptions { file_name: "/a.ts".to_string(), ..Default::default() };
    let node = f.new_source_file(opts, ";", statements, eof);
    set_parents(node);
    let file = node.as_source_file();
    assert_eq!(file.as_node(), node);
    assert_eq!(file.file_name(), "/a.ts");
    assert_eq!(file.text(), ";");
    assert_eq!(node.statements(), &[stmt]);
    assert_eq!(children(node), vec![stmt, eof]);
    assert_eq!(get_source_file_of_node(stmt).map(|f| f.as_node()), Some(node));
    file.symbol_count.set(3);
    assert_eq!(file.symbol_count(), 3);
    assert!(node.declaration_data().is_some());
    assert!(is_locals_container(node));
}

#[test]
fn reparser_mutations() {
    let f = NodeFactory::default();
    let name = f.new_identifier("x");
    let decl = f.new_variable_declaration(name, None, None, None);
    assert_eq!(decl.type_node(), None);
    let t = f.new_keyword_type_node(Kind::NumberKeyword);
    decl.as_mutable().set_type(Some(t));
    assert_eq!(decl.type_node(), Some(t));
    assert_eq!(decl.as_variable_declaration().type_(), Some(t));
    assert_eq!(children(decl), vec![name, t]);

    let m = f.new_modifier(Kind::ExportKeyword);
    decl.set_modifiers(None);
    let stmt_list = f.new_node_list(vec![decl]);
    let list = f.new_variable_declaration_list(stmt_list, NodeFlags::Const);
    assert!(list.flags().contains(NodeFlags::Const));
    let mods = f.new_modifier_list(vec![m]);
    let var_stmt = f.new_variable_statement(None, list);
    var_stmt.set_modifiers(Some(mods));
    assert_eq!(var_stmt.modifier_flags(), ModifierFlags::Export);
    assert_eq!(children(var_stmt), vec![m, list]);
}

#[test]
fn clone_and_visit_each_child() {
    let mut f = NodeFactory::default();
    let a = f.new_identifier("a");
    let plus = f.new_token(Kind::PlusToken);
    let b = f.new_identifier("b");
    let bin = f.new_binary_expression(None, a, None, plus, b);
    bin.set_loc(TextRange::new(3, 8));

    let copy = bin.clone_node(&f);
    assert_ne!(copy, bin);
    assert_eq!(copy.loc(), bin.loc());
    assert_eq!(copy.as_binary_expression().left(), a);

    // Identity visitor: nothing changes.
    let mut v = new_node_visitor(
        Some(Rc::new(|_: &mut NodeVisitor, n: P<Node>| Some(n))),
        None,
        NodeVisitorHooks::default(),
    );
    assert_eq!(v.visit_each_child(Some(bin)), Some(bin));

    // Replacing a child produces an updated node with the original's location.
    let c: P<Node> = v.factory.new_identifier("c");
    let mut v = new_node_visitor(
        Some(Rc::new(
            move |_: &mut NodeVisitor, n: P<Node>| if n == a { Some(c) } else { Some(n) },
        )),
        Some(std::mem::take(&mut f)),
        NodeVisitorHooks::default(),
    );
    let updated = v.visit_each_child(Some(bin)).unwrap();
    assert_ne!(updated, bin);
    assert_eq!(updated.loc(), TextRange::new(3, 8));
    assert_eq!(updated.as_binary_expression().left(), c);
    assert_eq!(updated.as_binary_expression().right(), b);
}

#[test]
fn symbol_table_order_and_snapshot() {
    let table = SymbolTable::new();
    let s1 = Symbol::new(SymbolFlags::Variable, "b");
    let s2 = Symbol::new(SymbolFlags::Function, "a");
    table.set("b", s1);
    table.set("a", s2);
    assert_eq!(table.keys(), vec!["b", "a"]);
    table.for_each(|name, _| {
        if name == "b" {
            table.delete("a");
        }
    });
    assert_eq!(table.len(), 1);
    assert_eq!(table.lookup("b"), Some(s1));
    assert!(!table.has("a"));
    assert!(INTERNAL_SYMBOL_NAME_CALL.starts_with(INTERNAL_SYMBOL_NAME_PREFIX));
    assert_eq!(escape_symbol_name(INTERNAL_SYMBOL_NAME_CALL), "__call");
}

// Regression: a table past the linear-search size keeps insertion order, keeps the original position on
// re-set and finds every remaining name after a delete shifts the hash index (wrong lookups would make the
// binder/checker miss or duplicate members).
#[test]
fn symbol_table_indexed_order_and_delete() {
    let names: Vec<&'static str> =
        (0..20).map(|i| &*Box::leak(format!("n{i}").into_boxed_str())).collect();
    let syms: Vec<P<Symbol>> =
        names.iter().map(|n| Symbol::new(SymbolFlags::Property, n)).collect();
    let table = SymbolTable::with_capacity(2);
    for (n, s) in names.iter().zip(&syms) {
        table.set(n, *s);
    }
    table.set(names[3], syms[0]);
    table.delete(names[5]);
    table.delete("missing");
    let mut expected: Vec<&str> = names.clone();
    expected.remove(5);
    assert_eq!(table.keys(), expected);
    assert_eq!(table.lookup(names[3]), Some(syms[0]));
    for (i, n) in names.iter().enumerate() {
        match i {
            3 => {}
            5 => assert_eq!(table.lookup(n), None),
            _ => assert_eq!(table.lookup(n), Some(syms[i])),
        }
    }
    let copy = table.clone_table();
    copy.set("new", syms[1]);
    assert_eq!(copy.len(), 20);
    assert_eq!(table.len(), 19);
    assert_eq!(copy.lookup(names[19]), Some(syms[19]));
    // Keys that are not their symbol's name ("n3" -> n0, "new" -> n1) survive deletes before them.
    assert_eq!(copy.lookup("new"), Some(syms[1]));
    copy.delete(names[1]);
    copy.delete(names[0]);
    assert_eq!(copy.lookup_entry(names[3]), Some((names[3], syms[0])));
    assert_eq!(copy.keys().last(), Some(&"new"));
    assert_eq!(copy.lookup("new"), Some(syms[1]));
    let small = SymbolTable::new();
    small.set("alias", syms[2]);
    small.set(names[4], syms[4]);
    small.delete("alias");
    assert_eq!(small.entries(), vec![(names[4], syms[4])]);
}

#[test]
fn symbol_table_long_names_share_the_capped_length() {
    // Entries store the key length capped at 63: keys longer than that are told apart by hash and text.
    let long = |c: char, n: usize| -> &'static str {
        Box::leak(format!("{}{c}", "x".repeat(n)).into_boxed_str())
    };
    let names: Vec<&'static str> =
        (0..40).map(|i| long(char::from(b'a' + (i % 26) as u8), 63 + i / 26)).collect();
    let syms: Vec<P<Symbol>> =
        names.iter().map(|n| Symbol::new(SymbolFlags::Property, n)).collect();
    for count in [5, 40] {
        let table = SymbolTable::new();
        for i in 0..count {
            table.set(names[i], syms[i]);
        }
        for i in 0..count {
            assert_eq!(table.lookup(names[i]), Some(syms[i]));
        }
        assert_eq!(table.lookup(long('z', 70)), None);
        table.delete(names[1]);
        assert_eq!(table.lookup(names[1]), None);
        assert_eq!(table.lookup(names[count - 1]), Some(syms[count - 1]));
    }
}

#[test]
fn kinds_and_flags() {
    assert_eq!(Kind::FIRST_KEYWORD, Kind::BreakKeyword);
    assert_eq!(Kind::LAST_TOKEN, Kind::SourceKeyword);
    assert_eq!(Kind::from_i16(Kind::Identifier as i16), Kind::Identifier);
    assert!(is_keyword_kind(Kind::ClassKeyword));
    assert!(is_assignment_operator(Kind::PlusEqualsToken));
    assert_eq!(NodeFlags::BlockScoped, NodeFlags::Let | NodeFlags::Const | NodeFlags::Using);
    assert!(SymbolFlags::Value.contains(SymbolFlags::Function));
    assert!(
        !SymbolFlags::FunctionScopedVariableExcludes
            .intersects(SymbolFlags::FunctionScopedVariable)
    );
    assert_eq!(std::mem::size_of::<Node>(), 24);
}

#[test]
fn node_header_word_keeps_kind_tag_and_parent() {
    let f = NodeFactory::default();
    let parent = f.new_keyword_expression(Kind::ThisKeyword);
    let child = f.new_token(Kind::Unknown);
    assert_eq!(
        (child.kind(), child.data_tag(), child.parent()),
        (Kind::Unknown, NodeDataTag::Token, None)
    );
    child.set_parent(Some(parent));
    assert_eq!(
        (child.kind(), child.data_tag(), child.parent()),
        (Kind::Unknown, NodeDataTag::Token, Some(parent))
    );
    assert_eq!(
        (parent.kind(), parent.data_tag()),
        (Kind::ThisKeyword, NodeDataTag::KeywordExpression)
    );
    child.set_parent(None);
    assert_eq!((child.kind(), child.parent()), (Kind::Unknown, None));
}
