use tsrs_core::CompilerOptions;
use tsrs_core::collections::OrderedMap;

use tsrs_tsoptions::gojson::*;

// A `paths` substitution that is not an array parses to Go's nil slice (TS5063, program.go:1153), which Go's
// encoding/json/v2 still marshals as `[]`; `null` unmarshals back to nil.
#[test]
fn nil_paths_substitutions_round_trip_like_go() {
    let mut paths = OrderedMap::default();
    paths.insert("@nil/*".to_string(), None);
    paths.insert("@empty/*".to_string(), Some(Vec::new()));
    let options = CompilerOptions { paths: Some(std::sync::Arc::new(paths)), ..Default::default() };
    assert_eq!(
        tsrs_core::json::marshal(&compiler_options_to_go_json(&options)).unwrap(),
        r#"{"paths":{"@nil/*":[],"@empty/*":[]}}"#
    );

    let back = compiler_options_from_go_json(
        &tsrs_core::json::unmarshal(r#"{"paths":{"@n/*":null,"@e/*":[]}}"#).unwrap(),
    )
    .unwrap();
    let paths = back.paths.unwrap();
    assert_eq!(paths.get("@n/*"), Some(&None));
    assert_eq!(paths.get("@e/*"), Some(&Some(Vec::new())));
}
