//! `glob.go`: Flamework's case-insensitive project-root glob pass and the `globs.json` artifact.

use std::collections::BTreeMap;

use rotor_rojo::PathTranslator;
use rotor_rojo::RbxPath;
use rotor_rojo::RojoResolver;
use rotor_support::gostr::quote;

use crate::json_syntax;
use crate::path::{project_path, translate_output_path};
use crate::{Error, ErrorKind};

/// `GlobPaths`: the runtime path-glob mapping emitted for one project.
pub type GlobPaths = BTreeMap<String, Vec<RbxPath>>;

/// Go's `path.ErrBadPattern`.
const BAD_PATTERN: &str = "syntax error in pattern";

/// `ExpandGlobs`: every regular file under `root` matching each pattern, case-insensitively,
/// translated through the project's path translator.
///
/// # Errors
///
/// A pattern escapes the root or is malformed, or the walk fails.
pub fn expand_globs(
	root: &str,
	translator: &PathTranslator,
	patterns: &[String],
) -> Result<BTreeMap<String, Vec<String>>, Error> {
	let mut clean_patterns = Vec::with_capacity(patterns.len());
	for pattern in patterns {
		clean_patterns.push(clean_glob_pattern(pattern)?);
	}

	let mut files = Vec::new();
	walk_regular_files(root, "", &mut files)
		.map_err(|error| Error::msg(format!("flamework: expand globs in {}: {error}", quote(root))))?;
	files.sort();

	let mut expanded = BTreeMap::new();
	for (index, pattern) in patterns.iter().enumerate() {
		let mut matches = Vec::new();
		for name in &files {
			let is_match = match_glob(&clean_patterns[index], name)
				.map_err(|message| Error::kinded(ErrorKind::InvalidGlob, &format!(" {}: {message}", quote(pattern))))?;
			if !is_match {
				continue;
			}
			let output = translate_output_path(root, translator, name).map_err(|error| {
				let message = format!(
					"flamework: translate glob {} match {}: {}",
					quote(pattern),
					quote(name),
					error.message()
				);
				error.reword(message)
			})?;
			matches.push(output);
		}
		expanded.insert(pattern.clone(), matches);
	}
	Ok(expanded)
}

/// `fs.WalkDir(os.DirFS(root), ".")`, keeping regular files: lexical order, symlinks not
/// followed, slash-separated names.
fn walk_regular_files(root: &str, relative: &str, files: &mut Vec<String>) -> Result<(), String> {
	let directory = if relative.is_empty() {
		root.to_owned()
	} else {
		rotor_support::gopath::join(&[root, &rotor_support::gopath::from_slash(relative)])
	};
	let entries = std::fs::read_dir(&directory).map_err(|error| {
		let shown = if relative.is_empty() { "." } else { relative };
		rotor_support::gostr::path_error("open", shown, &error)
	})?;
	let mut named = Vec::new();
	for entry in entries {
		let entry = entry.map_err(|error| rotor_support::gostr::io_error_text(&error))?;
		let file_type = entry
			.file_type()
			.map_err(|error| rotor_support::gostr::io_error_text(&error))?;
		named.push((entry.file_name().to_string_lossy().into_owned(), file_type));
	}
	named.sort_by(|left, right| left.0.cmp(&right.0));
	for (name, file_type) in named {
		let child = if relative.is_empty() {
			name
		} else {
			format!("{relative}/{name}")
		};
		if file_type.is_dir() {
			walk_regular_files(root, &child, files)?;
		} else if file_type.is_file() {
			files.push(child);
		}
	}
	Ok(())
}

/// `ResolveGlobPaths`: translated project-relative outputs as Rojo paths. Matches outside the
/// Rojo tree are omitted, as in upstream `convertGlobs`.
///
/// # Errors
///
/// A match escapes the root.
pub fn resolve_glob_paths(
	root: &str,
	resolver: Option<&RojoResolver>,
	expanded: &BTreeMap<String, Vec<String>>,
) -> Result<GlobPaths, Error> {
	let mut out = GlobPaths::new();
	for (pattern, matches) in expanded {
		let mut rbx_paths = Vec::with_capacity(matches.len());
		for matched in matches {
			let file_path = project_path(root, matched).map_err(|error| {
				let message = format!(
					"flamework: resolve glob {} match {}: {}",
					quote(pattern),
					quote(matched),
					error.message()
				);
				error.reword(message)
			})?;
			let Some(resolver) = resolver else {
				continue;
			};
			if let Some(rbx_path) = resolver.get_rbx_path_from_file_path(&file_path) {
				rbx_paths.push(rbx_path);
			}
		}
		out.insert(pattern.clone(), rbx_paths);
	}
	Ok(out)
}

/// One `GlobPaths` as `json.Marshal` writes it. Go copies each Rojo path with
/// `append(rojo.RbxPath(nil), ...)`, so an empty one is nil and encodes as `null`.
pub(crate) fn marshal_glob_paths(out: &mut String, paths: &GlobPaths) {
	out.push('{');
	for (index, (pattern, rbx_paths)) in paths.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		json_syntax::write_string(out, pattern, true);
		out.push_str(":[");
		for (path_index, rbx_path) in rbx_paths.iter().enumerate() {
			if path_index > 0 {
				out.push(',');
			}
			if rbx_path.is_empty() {
				out.push_str("null");
				continue;
			}
			out.push('[');
			for (segment_index, segment) in rbx_path.iter().enumerate() {
				if segment_index > 0 {
					out.push(',');
				}
				json_syntax::write_string(out, segment, true);
			}
			out.push(']');
		}
		out.push(']');
	}
	out.push('}');
}

/// `MarshalGlobsArtifact`: deterministic `include/flamework/globs.json` bytes, or `None` when
/// there is nothing to write.
#[must_use]
pub fn marshal_globs_artifact(game: Option<&GlobPaths>, packages: &BTreeMap<String, GlobPaths>) -> Option<Vec<u8>> {
	if game.is_none() && packages.is_empty() {
		return None;
	}
	let empty = GlobPaths::new();
	let mut out = String::from("{\"game\":");
	marshal_glob_paths(&mut out, game.unwrap_or(&empty));
	out.push_str(",\"packages\":{");
	for (index, (name, paths)) in packages.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		json_syntax::write_string(&mut out, name, true);
		out.push(':');
		marshal_glob_paths(&mut out, paths);
	}
	out.push_str("}}");
	Some(out.into_bytes())
}

pub(crate) fn clean_glob_pattern(pattern: &str) -> Result<String, Error> {
	let normalized = pattern.replace('\\', "/");
	if normalized.starts_with('/') {
		return Err(Error::kinded(ErrorKind::PathEscape, &format!(": {}", quote(pattern))));
	}
	let clean = path_clean(&normalized);
	if clean == ".." || clean.starts_with("../") {
		return Err(Error::kinded(ErrorKind::PathEscape, &format!(": {}", quote(pattern))));
	}
	let clean = clean.strip_prefix("./").unwrap_or(&clean).to_owned();
	for segment in clean.split('/') {
		if segment == "**" {
			continue;
		}
		if let Err(message) = path_match(segment.as_bytes(), b"") {
			return Err(Error::kinded(
				ErrorKind::InvalidGlob,
				&format!(" {}: {message}", quote(pattern)),
			));
		}
	}
	Ok(clean)
}

/// Go's `path.Clean` (slash-separated, no volumes).
fn path_clean(path: &str) -> String {
	rotor_support::gopath::Flavor::Unix.clean(path)
}

pub(crate) fn match_glob(pattern: &str, name: &str) -> Result<bool, &'static str> {
	let pattern = pattern.to_lowercase();
	let name = name.to_lowercase();
	let pattern: Vec<&str> = pattern.split('/').collect();
	let name: Vec<&str> = name.split('/').collect();
	match_glob_segments(&pattern, &name)
}

pub(crate) fn match_glob_segments(pattern: &[&str], name: &[&str]) -> Result<bool, &'static str> {
	let Some((&first, rest)) = pattern.split_first() else {
		return Ok(name.is_empty());
	};
	if first == "**" {
		let matched = match_glob_segments(rest, name)?;
		if matched || name.is_empty() || name[0].starts_with('.') {
			return Ok(matched);
		}
		return match_glob_segments(pattern, &name[1..]);
	}
	if name.is_empty() || (name[0].starts_with('.') && !first.starts_with('.')) {
		return Ok(false);
	}
	if !path_match(first.as_bytes(), name[0].as_bytes())? {
		return Ok(false);
	}
	match_glob_segments(rest, &name[1..])
}

/// Go's `path.Match`.
pub(crate) fn path_match(mut pattern: &[u8], mut name: &[u8]) -> Result<bool, &'static str> {
	'pattern: while !pattern.is_empty() {
		let (star, chunk, rest) = scan_chunk(pattern);
		pattern = rest;
		if star && chunk.is_empty() {
			return Ok(!name.contains(&b'/'));
		}
		let (t, ok, err) = match_chunk(chunk, name);
		if ok && (t.is_empty() || !pattern.is_empty()) {
			name = t;
			continue;
		}
		if let Some(err) = err {
			return Err(err);
		}
		if star {
			let mut i = 0;
			while i < name.len() && name[i] != b'/' {
				let (t, ok, err) = match_chunk(chunk, &name[i + 1..]);
				if ok {
					if pattern.is_empty() && !t.is_empty() {
						i += 1;
						continue;
					}
					name = t;
					continue 'pattern;
				}
				if let Some(err) = err {
					return Err(err);
				}
				i += 1;
			}
		}
		while !pattern.is_empty() {
			let (_, chunk, rest) = scan_chunk(pattern);
			pattern = rest;
			if let (_, _, Some(err)) = match_chunk(chunk, b"") {
				return Err(err);
			}
		}
		return Ok(false);
	}
	Ok(name.is_empty())
}

fn scan_chunk(mut pattern: &[u8]) -> (bool, &[u8], &[u8]) {
	let mut star = false;
	while pattern.first() == Some(&b'*') {
		pattern = &pattern[1..];
		star = true;
	}
	let mut in_range = false;
	let mut i = 0;
	while i < pattern.len() {
		match pattern[i] {
			b'\\' => {
				if i + 1 < pattern.len() {
					i += 1;
				}
			}
			b'[' => in_range = true,
			b']' => in_range = false,
			b'*' if !in_range => return (star, &pattern[..i], &pattern[i..]),
			_ => {}
		}
		i += 1;
	}
	(star, pattern, &[])
}

/// Go's `utf8.DecodeRuneInString`: the rune and its width; `U+FFFD` and 1 for invalid bytes.
fn decode_rune(bytes: &[u8]) -> (u32, usize) {
	if bytes.is_empty() {
		return (0xFFFD, 0);
	}
	let width = match bytes[0] {
		0x00..=0x7F => 1,
		0xC2..=0xDF => 2,
		0xE0..=0xEF => 3,
		0xF0..=0xF4 => 4,
		_ => return (0xFFFD, 1),
	};
	bytes
		.get(..width)
		.and_then(|slice| std::str::from_utf8(slice).ok())
		.map_or((0xFFFD, 1), |text| {
			(text.chars().next().map_or(0xFFFD, u32::from), width)
		})
}

type ChunkMatch<'a> = (&'a [u8], bool, Option<&'static str>);

fn match_chunk<'a>(mut chunk: &[u8], mut s: &'a [u8]) -> ChunkMatch<'a> {
	let mut failed = false;
	while !chunk.is_empty() {
		failed = failed || s.is_empty();
		match chunk[0] {
			b'[' => {
				let mut r = 0u32;
				if !failed {
					let (rune, n) = decode_rune(s);
					r = rune;
					s = &s[n..];
				}
				chunk = &chunk[1..];
				let mut negated = false;
				if chunk.first() == Some(&b'^') {
					negated = true;
					chunk = &chunk[1..];
				}
				let mut matched = false;
				let mut nrange = 0;
				loop {
					if chunk.first() == Some(&b']') && nrange > 0 {
						chunk = &chunk[1..];
						break;
					}
					let (lo, next) = match get_esc(chunk) {
						Ok(value) => value,
						Err(err) => return (&[], false, Some(err)),
					};
					chunk = next;
					let mut hi = lo;
					if chunk[0] == b'-' {
						let (high, next) = match get_esc(&chunk[1..]) {
							Ok(value) => value,
							Err(err) => return (&[], false, Some(err)),
						};
						hi = high;
						chunk = next;
					}
					matched = matched || (lo <= r && r <= hi);
					nrange += 1;
				}
				failed = failed || matched == negated;
			}
			b'?' => {
				if !failed {
					failed = s[0] == b'/';
					let (_, n) = decode_rune(s);
					s = &s[n..];
				}
				chunk = &chunk[1..];
			}
			first => {
				let literal = if first == b'\\' {
					chunk = &chunk[1..];
					if chunk.is_empty() {
						return (&[], false, Some(BAD_PATTERN));
					}
					chunk[0]
				} else {
					first
				};
				if !failed {
					failed = literal != s[0];
					s = &s[1..];
				}
				chunk = &chunk[1..];
			}
		}
	}
	if failed {
		return (&[], false, None);
	}
	(s, true, None)
}

fn get_esc(mut chunk: &[u8]) -> Result<(u32, &[u8]), &'static str> {
	if chunk.is_empty() || chunk[0] == b'-' || chunk[0] == b']' {
		return Err(BAD_PATTERN);
	}
	if chunk[0] == b'\\' {
		chunk = &chunk[1..];
		if chunk.is_empty() {
			return Err(BAD_PATTERN);
		}
	}
	let (r, n) = decode_rune(chunk);
	let next = &chunk[n..];
	if (r == 0xFFFD && n == 1) || next.is_empty() {
		return Err(BAD_PATTERN);
	}
	Ok((r, next))
}
