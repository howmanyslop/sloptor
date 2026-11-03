//! `sourcepositions.go`: UTF-16 source columns for origins.

use rotor_luau::Position;
use rotor_tsrs::scanner;

use crate::prelude::*;

/// At each completed non-ASCII character, the cumulative bytes in excess of UTF-16 code units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceColumnCheckpoint {
	byte_offset: i64,
	extra_bytes: i64,
}

fn clamp_u32(value: i64) -> u32 {
	u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

impl State<'_> {
	/// `sourcePositionAt`
	pub fn source_position_at(&mut self, file: P<SourceFile>, offset: i64) -> Position {
		let pos = clamp_u32(offset);
		if !self.source_map_enabled {
			let (line, column) = scanner::get_ecma_line_and_utf16_character_of_position(&*file, pos);
			return Position::new(
				clamp_u32(i64::try_from(line).unwrap_or(i64::MAX)),
				clamp_u32(column.into()),
			);
		}
		let checkpoints = self
			.source_map_column_indexes
			.entry(file)
			.or_insert_with(|| source_column_checkpoints(file.text()));
		let (line, column) = scanner::get_ecma_line_and_byte_offset_of_position(&*file, pos);
		let column = i64::from(column);
		let line_start = offset - column;
		let column = column
			- (source_column_adjustment(checkpoints, offset) - source_column_adjustment(checkpoints, line_start));
		Position::new(clamp_u32(i64::try_from(line).unwrap_or(i64::MAX)), clamp_u32(column))
	}
}

/// `sourceColumnCheckpoints`: non-empty only for non-ASCII text.
pub fn source_column_checkpoints(text: &str) -> Vec<SourceColumnCheckpoint> {
	let mut checkpoints = Vec::new();
	let mut extra_bytes = 0_i64;
	let bytes = text.as_bytes();
	let mut offset = 0;
	while offset < bytes.len() {
		if bytes[offset] < 0x80 {
			offset += 1;
			continue;
		}
		let (character, width) = decode_rune(&bytes[offset..]);
		offset += width;
		if width == 1 {
			continue; // An invalid UTF-8 byte is one replacement code unit.
		}
		extra_bytes += i64::try_from(width).unwrap_or(0) - 1;
		if character > 0xffff {
			extra_bytes -= 1; // A non-BMP character occupies two UTF-16 code units.
		}
		checkpoints.push(SourceColumnCheckpoint {
			byte_offset: i64::try_from(offset).unwrap_or(i64::MAX),
			extra_bytes,
		});
	}
	checkpoints
}

/// Go `utf8.DecodeRuneInString`: the character and its width, `(0xFFFD, 1)` when invalid.
fn decode_rune(bytes: &[u8]) -> (u32, usize) {
	let width = match bytes.first() {
		Some(0xC2..=0xDF) => 2,
		Some(0xE0..=0xEF) => 3,
		Some(0xF0..=0xF4) => 4,
		_ => return (0xFFFD, 1),
	};
	bytes
		.get(..width)
		.and_then(|slice| std::str::from_utf8(slice).ok())
		.and_then(|text| text.chars().next())
		.map_or((0xFFFD, 1), |character| (u32::from(character), width))
}

/// `sourceColumnAdjustment`
pub fn source_column_adjustment(checkpoints: &[SourceColumnCheckpoint], offset: i64) -> i64 {
	let index = checkpoints.partition_point(|checkpoint| checkpoint.byte_offset <= offset);
	if index == 0 {
		return 0;
	}
	checkpoints[index - 1].extra_bytes
}
