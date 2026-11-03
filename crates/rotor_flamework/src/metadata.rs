//! `metadata.go`: a file's `@metadata` token set.

use std::collections::BTreeSet;

/// `Metadata`: requested metadata tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
	tokens: BTreeSet<String>,
}

impl Metadata {
	/// `NewMetadata`
	#[must_use]
	pub fn new<I: IntoIterator<Item = S>, S: Into<String>>(tokens: I) -> Self {
		Self {
			tokens: tokens.into_iter().map(Into::into).collect(),
		}
	}

	/// `ParseMetadataText`: trims the text, collapses its first whitespace run to one space, and
	/// splits on spaces.
	#[must_use]
	pub fn parse_text(text: &str) -> Self {
		let trimmed = text.trim();
		let mut start: Option<usize> = None;
		let mut end = 0;
		for (index, character) in trimmed.char_indices() {
			if character.is_whitespace() {
				if start.is_none() {
					start = Some(index);
				}
				end = index + character.len_utf8();
				continue;
			}
			if start.is_some() {
				break;
			}
		}
		let joined = start.map_or_else(
			|| trimmed.to_owned(),
			|start| format!("{} {}", &trimmed[..start], &trimmed[end..]),
		);
		Self::new(joined.split(' '))
	}

	/// `Requested`: whether `name` is requested, by name or `*`, and not excluded by `~name`.
	#[must_use]
	pub fn requested(&self, name: &str) -> bool {
		if self.tokens.contains(&format!("~{name}")) {
			return false;
		}
		self.tokens.contains(name) || self.tokens.contains("*")
	}

	/// `Tokens`: the tokens, sorted.
	#[must_use]
	pub fn tokens(&self) -> Vec<String> {
		self.tokens.iter().cloned().collect()
	}
}
