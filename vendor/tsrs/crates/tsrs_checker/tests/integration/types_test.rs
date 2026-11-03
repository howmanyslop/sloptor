use tsrs_ast::Symbol;
use tsrs_ast::SymbolFlags;
use tsrs_checker::mapper::new_simple_type_mapper;
use tsrs_checker::*;
use tsrs_core::P;
use tsrs_core::alloc_vec;

#[test]
fn compact_type_node_links_keep_nil_distinct_from_empty() {
    let t1 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(1), IntrinsicType::default());
    let t2 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(2), IntrinsicType::default());
    let links = TypeNodeLinks::default();
    links.resolved_type.set(Some(t1));
    assert!(links.outer_type_parameters().is_none());
    links.set_outer_type_parameters(None);
    assert!(links.outer_type_parameters.get().is_none());

    links.set_outer_type_parameters(Some(&[]));
    assert_eq!(links.outer_type_parameters().unwrap().len(), 0);
    let header = links.outer_type_parameters.get().unwrap();
    let list = alloc_vec(vec![t1, t2]);
    links.set_outer_type_parameters(Some(list));
    assert_eq!(links.outer_type_parameters.get(), Some(header));
    assert_eq!(links.outer_type_parameters().unwrap().as_ptr(), list.as_ptr());
    for _ in 0..1024 {
        let other = TypeNodeLinks::default();
        other.set_outer_type_parameters(Some(list));
    }
    assert_eq!(links.outer_type_parameters(), Some(list));
    assert_eq!(links.resolved_type.get(), Some(t1));
    links.set_outer_type_parameters(None);
    assert!(links.outer_type_parameters().is_none());
    assert_eq!(links.resolved_type.get(), Some(t1));
}

#[test]
fn compact_signature_extra_keeps_optional_state_independent() {
    let t1 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(1), IntrinsicType::default());
    let t2 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(2), IntrinsicType::default());
    let no_predicate = P::new(TypePredicate::default());
    let predicate = P::new(TypePredicate::default());
    let sig = P::new(Signature::default());
    sig.flags.set(SignatureFlags::HasRestParameter);
    sig.resolved_return_type.set(Some(t1));
    assert!(sig.rare.tail().is_none());
    assert!(sig.type_parameters_option().is_none());
    assert_eq!(sig.type_parameters(), []);
    sig.set_type_parameters(None);
    sig.set_this_parameter(None);
    sig.set_composite(None);
    sig.set_resolved_type_predicate(None, no_predicate);
    assert!(sig.rare.tail().is_none());
    sig.set_resolved_type_predicate(Some(no_predicate), no_predicate);
    assert!(sig.rare.tail().is_none());
    assert_eq!(sig.resolved_type_predicate(no_predicate), Some(no_predicate));
    assert_eq!(sig.flags(), SignatureFlags::HasRestParameter);
    sig.set_resolved_type_predicate(None, no_predicate);

    sig.set_type_parameters(Some(&[]));
    assert!(sig.type_parameters_option().is_some());
    let tail = sig.rare.tail().unwrap();
    let list = alloc_vec(vec![t1, t2]);
    sig.set_type_parameters(Some(list));
    assert_eq!(sig.rare.tail(), Some(tail));
    assert_eq!(sig.type_parameters().as_ptr(), list.as_ptr());
    let this_parameter = Symbol::new(SymbolFlags::None, "");
    let composite = P::new(CompositeSignature::default());
    sig.set_this_parameter(Some(this_parameter));
    sig.set_composite(Some(composite));
    sig.set_isolated_signature_type(Some(t2));
    sig.set_resolved_type_predicate(Some(predicate), no_predicate);
    assert_eq!(sig.rare.tail(), Some(tail));

    let clone = P::new(Signature::default());
    clone.set_type_parameters(sig.type_parameters_option());
    clone.set_this_parameter(sig.this_parameter());
    clone.set_composite(sig.composite());
    assert_ne!(clone.rare.tail(), sig.rare.tail());
    assert_eq!(clone.type_parameters(), list);
    assert_eq!(clone.this_parameter(), Some(this_parameter));
    assert_eq!(clone.composite(), Some(composite));
    assert!(clone.isolated_signature_type().is_none());
    assert!(clone.resolved_type_predicate(no_predicate).is_none());
    clone.set_this_parameter(None);
    assert_eq!(sig.this_parameter(), Some(this_parameter));

    for _ in 0..1024 {
        let other = P::new(Signature::default());
        other.set_type_parameters(Some(list));
        other.set_resolved_type_predicate(Some(predicate), no_predicate);
    }
    assert_eq!(sig.rare.tail(), Some(tail));
    assert_eq!(sig.type_parameters(), list);
    assert_eq!(sig.isolated_signature_type(), Some(t2));
    assert_eq!(sig.resolved_type_predicate(no_predicate), Some(predicate));
    sig.set_type_parameters(None);
    sig.set_this_parameter(None);
    sig.set_composite(None);
    assert_eq!(sig.rare.tail(), Some(tail));
    assert!(sig.type_parameters_option().is_none());
    assert_eq!(sig.resolved_return_type.get(), Some(t1));
    assert_eq!(sig.flags(), SignatureFlags::HasRestParameter);
}

#[test]
fn compact_type_payload_casts_survive_arena_growth() {
    let mut next_id = 1;
    macro_rules! type_case {
        ($payload:ty, $cast:ident, $bases:expr) => {{
            let t = Type::alloc(
                TypeFlags::Any,
                ObjectFlags::None,
                TypeId(next_id),
                <$payload>::default(),
            );
            next_id += 1;
            let payload = std::ptr::from_ref(t.$cast()).addr();
            let expected = std::ptr::from_ref(t.get()).addr()
                + std::mem::offset_of!(TypeAlloc<$payload>, data);
            assert_eq!(payload, expected);
            assert_eq!(t.data_tag(), <$payload as TypePayload>::TAG);
            // The kind must not be inferred from mutable semantic flags.
            t.flags.set(TypeFlags::Never);
            assert_eq!(std::ptr::from_ref(t.$cast()).addr(), payload);
            (t, $bases)
        }};
    }

    // constrained, structured, object, reference, interface, union/intersection.
    // Rust keeps structured base constraints in the checker, rather than in the payload.
    let cases = [
        type_case!(IntrinsicType, as_intrinsic_type, [false, false, false, false, false, false]),
        type_case!(LiteralType, as_literal_type, [false, false, false, false, false, false]),
        type_case!(
            UniqueESSymbolType,
            as_unique_es_symbol_type,
            [false, false, false, false, false, false]
        ),
        type_case!(TypeParameter, as_type_parameter, [true, false, false, false, false, false]),
        type_case!(IndexType, as_index_type, [true, false, false, false, false, false]),
        type_case!(
            IndexedAccessType,
            as_indexed_access_type,
            [true, false, false, false, false, false]
        ),
        type_case!(
            TemplateLiteralType,
            as_template_literal_type,
            [true, false, false, false, false, false]
        ),
        type_case!(
            StringMappingType,
            as_string_mapping_type,
            [true, false, false, false, false, false]
        ),
        type_case!(
            SubstitutionType,
            as_substitution_type,
            [true, false, false, false, false, false]
        ),
        type_case!(ConditionalType, as_conditional_type, [true, false, false, false, false, false]),
        type_case!(ObjectType, as_object_type, [false, true, true, false, false, false]),
        type_case!(TypeReference, as_type_reference, [false, true, true, true, false, false]),
        type_case!(InterfaceType, as_interface_type, [false, true, true, true, true, false]),
        type_case!(TupleType, as_tuple_type, [false, true, true, true, true, false]),
        type_case!(
            InstantiationExpressionType,
            as_instantiation_expression_type,
            [false, true, true, false, false, false]
        ),
        type_case!(MappedType, as_mapped_type, [false, true, true, false, false, false]),
        type_case!(
            ReverseMappedType,
            as_reverse_mapped_type,
            [false, true, true, false, false, false]
        ),
        type_case!(
            EvolvingArrayType,
            as_evolving_array_type,
            [false, true, true, false, false, false]
        ),
        type_case!(UnionType, as_union_type, [false, true, false, false, false, true]),
        type_case!(
            IntersectionType,
            as_intersection_type,
            [false, true, false, false, false, true]
        ),
    ];
    for (t, expected) in cases {
        assert_eq!(
            [
                t.try_as_constrained_type().is_some(),
                t.try_as_structured_type().is_some(),
                t.try_as_object_type().is_some(),
                t.try_as_type_reference().is_some(),
                t.try_as_interface_type().is_some(),
                t.try_as_union_or_intersection_type().is_some(),
            ],
            expected,
            "{:?}",
            t.data_tag()
        );
    }

    let first = cases[0].0;
    let first_payload = std::ptr::from_ref(first.as_intrinsic_type());
    for _ in 0..1024 {
        let _ = Type::alloc(
            TypeFlags::Any,
            ObjectFlags::None,
            TypeId(next_id),
            IntrinsicType::default(),
        );
        next_id += 1;
    }
    assert_eq!(first.id(), TypeId(1));
    assert_eq!(std::ptr::from_ref(first.as_intrinsic_type()), first_payload);
}

#[test]
fn value_symbol_links_synthetic_mode_keeps_every_field() {
    let t1 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(1), IntrinsicType::default());
    let t2 = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(2), IntrinsicType::default());
    let s = Symbol::new(SymbolFlags::Property, "p");
    let m = new_simple_type_mapper(t1, t2);
    let fields = |l: &ValueSymbolLinks| {
        (l.target(), l.mapper(), l.containing_type(), l.name_type(), l.write_type())
    };

    // A synthetic property: containing and name type in the words of target and mapper, then a target moves them.
    let l = ValueSymbolLinks::default();
    l.set_containing_type(Some(t1));
    l.set_name_type(Some(t2));
    l.set_target(None);
    assert_eq!(fields(&l), (None, None, Some(t1), Some(t2), None));
    l.set_write_type(Some(t1));
    assert_eq!(fields(&l), (None, None, Some(t1), Some(t2), Some(t1)));
    l.set_mapper(Some(m));
    assert_eq!(fields(&l), (None, Some(m), Some(t1), Some(t2), Some(t1)));
    l.set_target(Some(s));
    l.set_containing_type(None);
    assert_eq!(fields(&l), (Some(s), Some(m), None, Some(t2), Some(t1)));

    // An instantiated symbol keeps containing/name types in the tail.
    let l = ValueSymbolLinks::default();
    l.set_target(Some(s));
    l.set_name_type(Some(t2));
    l.set_target(None);
    l.set_containing_type(Some(t1));
    assert_eq!(fields(&l), (None, None, Some(t1), Some(t2), None));
    l.set_mapper(Some(m));
    assert_eq!(fields(&l), (None, Some(m), Some(t1), Some(t2), None));
}
