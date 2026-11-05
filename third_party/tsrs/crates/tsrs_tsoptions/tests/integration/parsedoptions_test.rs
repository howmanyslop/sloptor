use tsrs_core::TypeAcquisition;

use tsrs_tsoptions::parsedoptions::*;

#[test]
fn type_acquisition_empty_lists_compare_equal() {
    let a = ParsedOptions {
        type_acquisition: Some(TypeAcquisition::default()),
        ..ParsedOptions::default()
    };
    let b = ParsedOptions {
        type_acquisition: Some(TypeAcquisition {
            include: Some(Vec::new()),
            exclude: Some(Vec::new()),
            ..TypeAcquisition::default()
        }),
        ..ParsedOptions::default()
    };
    assert!(a.equals(&b));
    assert!(b.equals(&a));
}
