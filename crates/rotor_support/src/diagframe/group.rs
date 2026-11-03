//! Groups of diagnostics rendered together with a summary footer.

use std::fmt::Write;

use super::{Language, Options, Spot, render, styler_for};
use crate::term::Styler;

/// One file's diagnostics, ready to render.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Group {
	/// The file path, shown in the locator and used to sort groups.
	pub path: String,
	/// The file's source text.
	pub source: String,
	/// Which keyword set to highlight.
	pub lang: Language,
	/// The diagnostics anchored in this file.
	pub spots: Vec<Spot>,
}

/// Renders all groups (files sorted by path), then a summary footer.
///
/// `max_frames` caps the total number of rendered diagnostic frames across all
/// groups, errors and warnings alike (0 means unlimited); when capped, the
/// footer notes how many were hidden.
#[must_use]
pub fn render_groups(groups: &[Group], o: Options, max_frames: usize) -> String {
	let mut sorted: Vec<&Group> = groups.iter().collect();
	sorted.sort_by(|a, b| a.path.cmp(&b.path));

	let mut out = String::new();
	let (mut errs, mut warns, mut shown, mut total, mut files_with_diags) = (0, 0, 0, 0, 0);
	for g in sorted {
		if !g.spots.is_empty() {
			files_with_diags += 1;
		}
		let mut to_render = Vec::new();
		for sp in &g.spots {
			total += 1;
			match sp.severity {
				super::Severity::Warning => warns += 1,
				super::Severity::Error => errs += 1,
			}
			if max_frames > 0 && shown >= max_frames {
				continue;
			}
			to_render.push(sp.clone());
			shown += 1;
		}
		if !to_render.is_empty() {
			out.push_str(&render(&g.path, &g.source, g.lang, &to_render, o));
			out.push('\n');
		}
	}

	let s = styler_for(o.color);
	let hidden = total - shown;
	if hidden > 0 {
		// Keep no-color output plain ASCII and byte-stable.
		let ellipsis = if o.color { "…" } else { "..." };
		let _ = writeln!(out, "  {}", s.muted(&format!("{ellipsis}and {hidden} more")));
	}
	out.push_str(&summary_line(s, errs, warns, files_with_diags));
	out
}

fn summary_line(s: Styler, errs: usize, warns: usize, files: usize) -> String {
	let mut parts = Vec::new();
	if errs > 0 {
		parts.push(s.error(&plural(errs, "error")));
	}
	if warns > 0 {
		parts.push(s.warn(&plural(warns, "warning")));
	}
	if parts.is_empty() {
		return String::new();
	}
	let g = s.glyphs();
	let glyph = if errs == 0 {
		s.warn_bold(g.warn)
	} else {
		s.error_bold(g.cross)
	};
	// Keep no-color output plain ASCII and byte-stable.
	let sep = if s.color() { " · " } else { ", " };
	format!(
		"  {glyph} {} {}\n",
		parts.join(sep),
		s.muted(&format!("in {}", plural(files, "file")))
	)
}

/// Renders "1 error" / "3 errors".
fn plural(n: usize, word: &str) -> String {
	if n == 1 {
		format!("{n} {word}")
	} else {
		format!("{n} {word}s")
	}
}

#[cfg(test)]
mod tests;
