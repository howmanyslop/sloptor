use tsrs_checker::checker_09::*;
use tsrs_checker::*;
use tsrs_core::P;

fn types(count: usize) -> Vec<P<Type>> {
    (0..count)
        .map(|i| {
            Type::alloc(
                TypeFlags::Any,
                ObjectFlags::None,
                TypeId(u32::try_from(i + 1).unwrap()),
                IntrinsicType::default(),
            )
        })
        .collect()
}

#[test]
fn type_list_keys_keep_the_go_byte_stream_across_spills() {
    for count in [0, 1, 46, 47, 256, 4096] {
        let types = types(count);
        let prefix = "x".repeat(count);
        let mut expected = prefix.as_bytes().to_vec();
        expected.push(17);
        expected.extend_from_slice(&123u32.to_le_bytes());
        expected.extend_from_slice(&(count as u64).to_le_bytes());
        for t in &types {
            expected.extend_from_slice(&t.id.0.to_le_bytes());
        }
        expected.extend_from_slice(b"suffix");

        let mut builder = KeyBuilder::default();
        builder.write_string(&prefix);
        builder.write_byte(17);
        builder.write_uint32(123);
        builder.write_types(&types);
        builder.write_string("suffix");
        let hash = CacheHashKey::hash_128(&expected);
        assert_eq!(builder.hash(), hash, "type count {count}");
        assert_eq!(builder.hash(), hash, "repeat hash, type count {count}");

        builder.write_byte(b'!');
        expected.push(b'!');
        assert_eq!(builder.hash(), CacheHashKey::hash_128(&expected));
    }
}

#[test]
fn type_list_keys_reserve_before_spilling() {
    for count in [0, 1, 8, 46, 47, 256, 4096] {
        let types = types(count);
        let size = 8 + 4 * count;
        let mut builder = KeyBuilder::default();
        builder.write_types(&types);
        assert_eq!(builder.overflow_buffer.is_some(), size > builder.inline_buffer.len());
        let pointer = builder.overflow_buffer.as_ref().map(Vec::as_ptr);
        if let Some(overflow) = &builder.overflow_buffer {
            assert!(overflow.capacity() >= size, "type count {count}");
        }
        let mut expected = Vec::with_capacity(size);
        expected.extend_from_slice(&(count as u64).to_le_bytes());
        for t in &types {
            expected.extend_from_slice(&t.id.0.to_le_bytes());
        }
        assert_eq!(builder.hash(), CacheHashKey::hash_128(&expected));
        assert_eq!(builder.overflow_buffer.as_ref().map(Vec::as_ptr), pointer);
        assert_eq!(builder.hash(), CacheHashKey::hash_128(&expected));
        assert_eq!(get_type_list_key(&types), CacheHashKey::hash_128(&expected));
    }
}
