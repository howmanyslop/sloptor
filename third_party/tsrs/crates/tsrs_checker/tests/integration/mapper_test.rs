use tsrs_checker::inference::new_inference_info;
use tsrs_checker::*;
use tsrs_core::alloc_slice;

#[test]
fn compact_mapper_kinds_and_payloads_survive_arena_growth() {
    assert_eq!(std::mem::size_of::<TypeMapper>(), 16);
    let source = Type::alloc(
        TypeFlags::TypeParameter,
        ObjectFlags::None,
        TypeId(1),
        TypeParameter::default(),
    );
    let target =
        Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(2), IntrinsicType::default());
    let other = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(3), IntrinsicType::default());
    source.as_type_parameter().is_this_type.set(true);

    let simple = new_simple_type_mapper(source, target);
    let array =
        new_array_type_mapper(alloc_slice(&[source, other]), alloc_slice(&[target, source]));
    let this_array = new_array_type_mapper(alloc_slice(&[source]), alloc_slice(&[target]));
    let merged = new_merged_type_mapper(simple, array);
    let array_to_single = new_array_to_single_type_mapper(alloc_slice(&[source]), target);
    let deferred = new_deferred_type_mapper(
        alloc_slice(&[source]),
        vec![Box::new(move |_: &mut Checker| target)],
    );
    let function = new_function_type_mapper(|_, t| t);
    let composite = new_composite_type_mapper(array, simple);

    assert_eq!(simple.kind(), TypeMapperKind::Simple);
    assert_eq!(array.kind(), TypeMapperKind::Array);
    assert_eq!(merged.kind(), TypeMapperKind::Merged);
    for mapper in [array_to_single, deferred, function, composite] {
        assert_eq!(mapper.kind(), TypeMapperKind::Unknown);
    }
    for mapper in [simple, this_array, array_to_single, deferred] {
        assert!(mapper.maps_this_only());
    }
    for mapper in [array, merged, function, composite] {
        assert!(!mapper.maps_this_only());
    }

    for i in 4..1028 {
        let t = Type::alloc(TypeFlags::Any, ObjectFlags::None, TypeId(i), IntrinsicType::default());
        new_simple_type_mapper(t, other);
    }
    assert!(
        matches!(simple.data(), TypeMapperData::Simple { source: s, target: t } if s == source && t == target)
    );
    assert!(
        matches!(array.data(), TypeMapperData::Array { sources, targets } if sources == [source, other] && targets == [target, source])
    );
    assert!(
        matches!(merged.data(), TypeMapperData::Merged { m1, m2 } if m1 == simple && m2 == array)
    );
    assert!(
        matches!(array_to_single.data(), TypeMapperData::ArrayToSingle { sources, target: t } if sources == [source] && t == target)
    );
    assert!(matches!(deferred.data(), TypeMapperData::Deferred { .. }));
    assert!(matches!(function.data(), TypeMapperData::Function { .. }));
    assert!(
        matches!(composite.data(), TypeMapperData::Composite { m1, m2 } if m1 == array && m2 == simple)
    );
}

#[test]
fn inference_mappers_keep_identity_and_context_alive() {
    let parameter = Type::alloc(
        TypeFlags::TypeParameter,
        ObjectFlags::None,
        TypeId(1),
        TypeParameter::default(),
    );
    let info = new_inference_info(parameter);
    let context = Checker::new_inference_context_worker(
        &[info],
        None,
        InferenceFlags::None,
        &|_, _, _, _| Ternary::TRUE,
    );
    if tsrs_core::lazymembers::lazy_inference_mappers() {
        assert_eq!(context.own_mappers(), [None, None]);
    }
    let fixing = context.mapper().unwrap();
    let non_fixing = context.non_fixing_mapper().unwrap();
    assert_ne!(fixing, non_fixing);
    assert_eq!(context.mapper(), Some(fixing));
    assert_eq!(context.non_fixing_mapper(), Some(non_fixing));
    assert!(matches!(fixing.data(), TypeMapperData::Inference { n, fixing: true } if n == context));
    assert!(
        matches!(non_fixing.data(), TypeMapperData::Inference { n, fixing: false } if n == context)
    );

    let clone = Checker::clone_inference_context(Some(context), InferenceFlags::None).unwrap();
    assert_ne!(clone.inferences.get()[0], info);
    assert_eq!(clone.inferences.get()[0].type_parameter.get(), Some(parameter));
    assert_ne!(clone.mapper().unwrap(), fixing);
    escape_mapper(fixing);
    assert!(context.escaped());
    assert!(non_fixing.escaped());
    InferenceContext::recycle(context);
    InferenceContext::recycle(clone);
    for _ in 0..1024 {
        let disposable = Checker::new_inference_context_worker(
            &[new_inference_info(parameter)],
            None,
            InferenceFlags::None,
            &|_, _, _, _| Ternary::TRUE,
        );
        InferenceContext::recycle(disposable);
    }
    assert!(matches!(fixing.data(), TypeMapperData::Inference { n, fixing: true } if n == context));
    assert!(
        matches!(non_fixing.data(), TypeMapperData::Inference { n, fixing: false } if n == context)
    );
}
