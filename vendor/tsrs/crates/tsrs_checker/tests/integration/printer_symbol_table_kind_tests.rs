use tsrs_checker::printer::*;

#[test]
fn mask_extracts_every_table_kind_without_source_id_bits() {
    let source_id = 0x12345;
    for kind in [
        ST_KIND_LOCALS,
        ST_KIND_EXPORTS,
        ST_KIND_MEMBERS,
        ST_KIND_GLOBALS,
        ST_KIND_RESOLVED_EXPORTS,
    ] {
        assert_eq!((kind | source_id) & ST_KIND_MASK, kind);
    }
}
