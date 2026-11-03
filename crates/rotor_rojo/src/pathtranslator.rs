//! Port of `@roblox-ts/path-translator` 1.1.0
//! (`reference/path-translator/src/PathTranslator.ts`; line references are
//! into that file). Pure path and string code with no filesystem access.

use rotor_support::gopath;

use crate::resolver::{INIT_NAME, LUA_EXT, LUAU_EXT};

// Extension and name constants (path-translator `src/constants.ts`).
const TS_EXT: &str = ".ts";
const TSX_EXT: &str = ".tsx";
const D_EXT: &str = ".d";
const DTS_EXT: &str = ".d.ts";
const TRANSFORMED_EXT: &str = ".transformed";
const BUILD_INFO_EXT: &str = ".tsbuildinfo";
const RBXTSC_INFIX: &str = ".rbxtsc";

const INDEX_NAME: &str = "index";

/// A path as a file name plus a stack of all its dot-extensions, so
/// `a.spec.ts` has exts `[".spec", ".ts"]` (`PathTranslator.ts` L16-39).
struct PathInfo {
	dir_name: String,
	file_name: String,
	exts: Vec<String>,
}

impl PathInfo {
	fn from(file_path: &str) -> Self {
		let dir_name = gopath::dir(file_path);
		// Upstream slices past dirName.length + 1 unconditionally; replicate
		// (including the quirk of eating a character when dirName is not a
		// strict "<dir><sep>" prefix, e.g. bare relative file names).
		let start = dir_name.len() + 1;
		let rest = if start < file_path.len() {
			String::from_utf8_lossy(&file_path.as_bytes()[start..]).into_owned()
		} else {
			String::new()
		};
		let mut parts = rest.split('.');
		let file_name = parts.next().unwrap_or_default().to_owned();
		let exts = parts.map(|v| format!(".{v}")).collect();
		Self {
			dir_name,
			file_name,
			exts,
		}
	}

	/// The ext `depth` entries from the top of the stack, empty when out of
	/// range (JS `undefined`).
	fn exts_peek(&self, depth: usize) -> &str {
		self.exts.len().checked_sub(depth + 1).map_or("", |i| &self.exts[i])
	}

	fn pop_ext(&mut self) -> String {
		self.exts.pop().unwrap_or_default()
	}

	fn push_ext(&mut self, ext: &str) {
		self.exts.push(ext.to_owned());
	}

	fn join(&self) -> String {
		gopath::join(&[&self.dir_name, &format!("{}{}", self.file_name, self.exts.concat())])
	}
}

/// Maps between input (`.ts`) and output (`.luau`) paths
/// (`PathTranslator.ts` L41-220).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathTranslator {
	/// The TypeScript `rootDir`.
	pub root_dir: String,
	/// The TypeScript `outDir`.
	pub out_dir: String,
	/// The build-info output path with the `.rbxtsc` infix added; empty when
	/// there is none.
	pub build_info_output_path: String,
	/// Whether declaration files are emitted.
	pub declaration: bool,
	/// Whether output files use `.luau` instead of `.lua`.
	pub use_luau_extension: bool,
}

impl PathTranslator {
	/// Mirrors the upstream constructor. `build_info_output_path` may be empty
	/// (upstream `undefined`).
	#[must_use]
	pub fn new(
		root_dir: &str,
		out_dir: &str,
		build_info_output_path: &str,
		declaration: bool,
		use_luau_extension: bool,
	) -> Self {
		Self {
			root_dir: root_dir.to_owned(),
			out_dir: out_dir.to_owned(),
			build_info_output_path: add_rbxtsc_build_info_infix(build_info_output_path),
			declaration,
			use_luau_extension,
		}
	}

	const fn luau_ext(&self) -> &'static str {
		if self.use_luau_extension { LUAU_EXT } else { LUA_EXT }
	}

	/// Maps an input path to an output path (`PathTranslator.ts` L58-80):
	///
	/// - `.ts(x)` and not `.d.ts(x)` becomes `.lua(u)`, with `index` becoming
	///   `init`
	/// - `src/*` becomes `out/*`
	#[must_use]
	pub fn get_output_path(&self, file_path: &str) -> String {
		let mut p = PathInfo::from(file_path);

		if (p.exts_peek(0) == TS_EXT || p.exts_peek(0) == TSX_EXT) && p.exts_peek(1) != D_EXT {
			p.pop_ext(); // pop .ts(x)

			// index -> init
			if p.file_name == INDEX_NAME {
				INIT_NAME.clone_into(&mut p.file_name);
			}

			p.push_ext(self.luau_ext());
		}

		make_relative(&self.root_dir, &self.out_dir, &p)
	}

	/// Maps an input path to an output `.d.ts` path (`PathTranslator.ts`
	/// L82-97).
	#[must_use]
	pub fn get_output_declaration_path(&self, file_path: &str) -> String {
		let mut p = PathInfo::from(file_path);

		if (p.exts_peek(0) == TS_EXT || p.exts_peek(0) == TSX_EXT) && p.exts_peek(1) != D_EXT {
			p.pop_ext(); // pop .ts(x)
			p.push_ext(DTS_EXT);
		}

		make_relative(&self.root_dir, &self.out_dir, &p)
	}

	/// Maps an input path to an output `.transformed.ts(x)` path
	/// (`PathTranslator.ts` L99-118).
	#[must_use]
	pub fn get_output_transformed_path(&self, file_path: &str) -> String {
		let mut p = PathInfo::from(file_path);

		// Transformers currently never get a chance to transform .d.ts files,
		// but the case is covered anyway.
		let from_end = if p.exts_peek(1) == D_EXT { 2 } else { 1 };
		// JS splice clamps negative indices to the start.
		let at = p.exts.len().saturating_sub(from_end);
		p.exts.insert(at, TRANSFORMED_EXT.to_owned());

		make_relative(&self.root_dir, &self.out_dir, &p)
	}

	/// Maps an output path back to its possible input paths
	/// (`PathTranslator.ts` L120-189): `.lua(u)` to `.ts(x)` (with `init` to
	/// `index`), `.d.ts(x)` to `.ts(x)` when `declaration`, plus identity.
	#[must_use]
	pub fn get_input_paths(&self, file_path: &str) -> Vec<String> {
		let mut possible_paths = Vec::new();
		let mut p = PathInfo::from(file_path);
		let to_root = |p: &PathInfo| make_relative(&self.out_dir, &self.root_dir, p);

		// index.*.lua(u) cannot come from a .ts file
		if p.exts_peek(0) == self.luau_ext() && p.file_name != INDEX_NAME {
			p.pop_ext(); // pop .lua(u)

			// ts
			p.push_ext(TS_EXT);
			possible_paths.push(to_root(&p));
			p.pop_ext();

			// tsx
			p.push_ext(TSX_EXT);
			possible_paths.push(to_root(&p));
			p.pop_ext();

			// init -> index
			if p.file_name == INIT_NAME {
				let original_file_name = std::mem::replace(&mut p.file_name, INDEX_NAME.to_owned());

				// index.*.ts
				p.push_ext(TS_EXT);
				possible_paths.push(to_root(&p));
				p.pop_ext();

				// index.*.tsx
				p.push_ext(TSX_EXT);
				possible_paths.push(to_root(&p));
				p.pop_ext();

				p.file_name = original_file_name;
			}

			p.push_ext(self.luau_ext());
		}

		if self.declaration && (p.exts_peek(0) == TS_EXT || p.exts_peek(0) == TSX_EXT) && p.exts_peek(1) == D_EXT {
			let ts_like_ext = p.pop_ext(); // pop .ts(x)
			p.pop_ext(); // pop .d

			// .ts
			p.push_ext(TS_EXT);
			possible_paths.push(to_root(&p));
			p.pop_ext();

			// .tsx
			p.push_ext(TSX_EXT);
			possible_paths.push(to_root(&p));
			p.pop_ext();

			p.push_ext(D_EXT);
			p.push_ext(&ts_like_ext);
		}

		possible_paths.push(to_root(&p));
		possible_paths
	}

	/// Maps a src path to an import path (`PathTranslator.ts` L191-219).
	/// Import paths are passed to `RojoResolver` and used virtually to resolve
	/// `RbxPath`s, so the result may not actually exist on disk.
	///
	/// - `.d.ts(x)` becomes `.ts(x)` becomes `.lua(u)`, with `index` becoming
	///   `init`
	///
	/// When `is_node_module` is true the compiled file is assumed to be a
	/// sibling of `file_path`, with no `rootDir` to `outDir` rebase:
	/// `node_modules` `.d.ts` files were already remapped to the shipped `.lua`
	/// location (`nodeModulesPathMapping`).
	#[must_use]
	pub fn get_import_path(&self, file_path: &str, is_node_module: bool) -> String {
		let mut p = PathInfo::from(file_path);

		if p.exts_peek(0) == TS_EXT || p.exts_peek(0) == TSX_EXT {
			p.pop_ext(); // pop .ts(x)
			if p.exts_peek(0) == D_EXT {
				p.pop_ext(); // pop .d
			}

			// index -> init
			if p.file_name == INDEX_NAME {
				INIT_NAME.clone_into(&mut p.file_name);
			}

			p.push_ext(self.luau_ext()); // push .lua(u)
		}

		if is_node_module {
			return p.join();
		}
		make_relative(&self.root_dir, &self.out_dir, &p)
	}
}

fn add_rbxtsc_build_info_infix(file_path: &str) -> String {
	let Some(without_ext) = file_path.strip_suffix(BUILD_INFO_EXT) else {
		return file_path.to_owned();
	};
	if without_ext.ends_with(RBXTSC_INFIX) {
		return file_path.to_owned();
	}
	format!("{without_ext}{RBXTSC_INFIX}{BUILD_INFO_EXT}")
}

/// Rebases a path from one root into another (`makeRelativeFactory`,
/// `PathTranslator.ts` L54-56).
fn make_relative(from: &str, to: &str, p: &PathInfo) -> String {
	let joined = p.join();
	// Node's path.relative returns the target itself when no relative path
	// exists (e.g. different drives); join then concatenates.
	let rel = gopath::rel(from, &joined).unwrap_or(joined);
	gopath::join(&[to, &rel])
}

#[cfg(test)]
mod tests;
