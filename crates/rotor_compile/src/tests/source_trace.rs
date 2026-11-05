//! `source_trace_test.go`

use super::{SourcePosition, SourceTraceMap, TraceMapping};
use crate::source_trace_sourcemap::{EmittedMapping, encode_emitted_mappings};

const fn mapping(generated_line: u32, generated_column: u32, source_line: u32, source_column: u32) -> TraceMapping {
	TraceMapping {
		generated_line,
		generated_column,
		source_line,
		source_column,
	}
}

#[test]
fn new_source_trace_map_decodes_mappings_in_generated_position_order() {
	let raw = r#"{"version":3,"file":"transformed.ts","sources":["original.ts"],"sourcesContent":["first\nsecond\n"],"mappings":"AAAA,KAAI;AACJ,Q"}"#;

	let trace = rotor_tsrs::with_large_stack(|| SourceTraceMap::new(raw, "original.ts", "first\nsecond\n"));

	let trace = trace.unwrap_or_else(|error| panic!("{error}"));
	let want = [mapping(0, 0, 0, 0), mapping(0, 5, 0, 4), mapping(1, 0, 1, 0)];
	assert_eq!(trace.mappings, want, "decoded mappings");
	for pair in trace.mappings.windows(2) {
		let (previous, current) = (pair[0], pair[1]);
		assert!(
			!(current.generated_line < previous.generated_line
				|| current.generated_line == previous.generated_line
					&& current.generated_column < previous.generated_column),
			"mappings are not ordered for sort.Search: {:?}",
			trace.mappings
		);
	}
}

/// Go's "before first mapping" row asks for column -1, which a `usize` column cannot spell; the
/// remaining rows port as they are.
#[test]
fn source_trace_map_original_position_uses_preceding_generated_mapping() {
	let trace = SourceTraceMap {
		mappings: vec![mapping(0, 0, 0, 0), mapping(0, 5, 0, 4), mapping(1, 0, 1, 0)],
		..SourceTraceMap::default()
	};
	let position = |line, column| SourcePosition { line, column };

	let tests = [
		("between mappings", position(0, 3), Some(position(0, 0))),
		("at mapping", position(0, 5), Some(position(0, 4))),
		("next generated line", position(1, 0), Some(position(1, 0))),
	];
	for (name, input, want) in tests {
		assert_eq!(
			trace.original_position(input),
			want,
			"{name}: OriginalPosition({input:?})"
		);
	}
}

#[test]
fn encode_emitted_mappings_uses_stable_vlq() {
	let source = |line, column| Some(SourcePosition { line, column });
	let mappings = [
		EmittedMapping {
			generated_line: 0,
			generated_column: 0,
			source: source(0, 0),
		},
		EmittedMapping {
			generated_line: 0,
			generated_column: 5,
			source: source(0, 4),
		},
		EmittedMapping {
			generated_line: 1,
			generated_column: 0,
			source: source(1, 0),
		},
		EmittedMapping {
			generated_line: 1,
			generated_column: 8,
			source: None,
		},
	];

	assert_eq!(
		encode_emitted_mappings(&mappings),
		"AAAA,KAAI;AACJ,Q",
		"encoded mappings"
	);
}
