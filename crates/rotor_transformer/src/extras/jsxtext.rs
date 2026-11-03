//! `jsxtext.go`: `util/fixupWhitespaceAndDecodeEntities.ts`, itself TypeScript's own JSX
//! transformer algorithm, copied from tsgo's unexported implementation
//! (`transformers/jsxtransforms/jsx.go`).
//!
//! Known micro-divergences from the TS original, both unreachable for byte-parity files:
//! - A lone-surrogate numeric entity (`&#xD800;`) renders U+FFFD where JS keeps the unpaired
//!   surrogate.
//! - `decodeEntities`' ampersand re-scan handles `&&amp;;`-style nesting slightly differently
//!   from the TS regex-replace.
//!
//! The roblox-ts backslash doubling (`transformJsxChildren.ts` L34) lives at the call site in
//! `jsx.rs`.

/// `stringutil.IsLineBreak`
const fn is_line_break(character: char) -> bool {
	matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `stringutil.IsWhiteSpaceSingleLine`
const fn is_white_space_single_line(character: char) -> bool {
	matches!(
		character,
		' ' | '\t' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{A0}' | '\u{1680}' | '\u{2000}'
			..='\u{200B}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
	)
}

fn add_line_of_jsx_text(builder: &mut String, trimmed_line: &str, is_initial: bool) {
	// We do not escape the string here as that is handled by the printer when it emits the
	// literal. We do, however, need to decode JSX entities.
	let decoded = decode_entities(trimmed_line);
	if !is_initial {
		builder.push(' ');
	}
	builder.push_str(&decoded);
}

/// `fixupWhitespaceAndDecodeEntities`: JSX trims whitespace at the end and beginning of lines,
/// except that the start/end of a tag is considered a start/end of a line only if that line is
/// on the same line as the closing tag.
///
/// - If there is only one line, return it.
/// - If there is only whitespace (but multiple lines), return `""` (TS returns `undefined`,
///   which upstream's `?? ""` folds the same way).
/// - Otherwise 'trimRight' the first line, 'trimLeft' the last line, 'trim' middle lines,
///   decode entities on each line, drop empty lines and join the rest with " ".
pub(super) fn fixup_whitespace_and_decode_entities(text: &str) -> String {
	let mut acc = String::new();
	let mut initial = true;
	// First non-whitespace character on this line.
	let mut first_non_whitespace: Option<usize> = Some(0);
	// End byte position (exclusive) of the last non-whitespace character on this line.
	let mut last_non_whitespace_end: Option<usize> = None;
	// These initial values are special because the first line is: first_non_whitespace = 0 to
	// indicate that we want leading whitespace, but last_non_whitespace_end = None as a special
	// flag to indicate that we *don't* include the line if it's all whitespace.
	for (index, character) in text.char_indices() {
		if is_line_break(character) {
			// If we've seen any non-whitespace characters on this line, add the 'trim' of the
			// line.
			if let (Some(first), Some(last)) = (first_non_whitespace, last_non_whitespace_end) {
				add_line_of_jsx_text(&mut acc, &text[first..last], initial);
				initial = false;
			}

			// Reset first_non_whitespace for the next line. Don't bother to reset
			// last_non_whitespace_end because we ignore it if first_non_whitespace is None.
			first_non_whitespace = None;
		} else if !is_white_space_single_line(character) {
			last_non_whitespace_end = Some(index + character.len_utf8());
			if first_non_whitespace.is_none() {
				first_non_whitespace = Some(index);
			}
		}
	}

	if let Some(first) = first_non_whitespace {
		// Last line had a non-whitespace character. Emit the 'trimLeft', meaning keep trailing
		// whitespace.
		add_line_of_jsx_text(&mut acc, &text[first..], initial);
	}
	acc
}

/// `decodeEntities`: replaces entities like `&nbsp;`, `&#123;` and `&#xDEADBEEF;` with the
/// characters they encode.
pub(super) fn decode_entities(text: &str) -> String {
	let Some(mut index) = text.find('&') else {
		return text.to_owned();
	};

	let mut text = text;
	let mut result = String::with_capacity(text.len());
	loop {
		result.push_str(&text[..index]);
		text = &text[index..];

		let Some(mut semi) = text.find(';') else {
			break;
		};

		// Skip past any intervening '&' characters between the current '&' and the ';'. Each such
		// '&' is not part of a valid entity, so emit it (and any text before the next '&') as
		// literals.
		while let Some(next_amp) = text[1..semi].find('&') {
			result.push_str(&text[..=next_amp]);
			text = &text[next_amp + 1..];
			semi -= next_amp + 1;
		}

		let entity = &text[1..semi];
		if let Some(decoded) = decode_entity(entity) {
			result.push(decoded);
		} else {
			result.push_str(&text[..=semi]);
		}
		text = &text[semi + 1..];

		let Some(next) = text.find('&') else {
			break;
		};
		index = next;
	}
	result.push_str(text);
	result
}

fn decode_entity(entity: &str) -> Option<char> {
	if entity.is_empty() {
		return None;
	}

	if let Some(numeric) = entity.strip_prefix('#') {
		if numeric.is_empty() {
			return None;
		}

		let (radix, digits) = numeric.strip_prefix('x').map_or((10, numeric), |hex| (16, hex));

		if digits.is_empty() {
			return None;
		}

		let valid = digits.chars().all(|character| {
			if radix == 16 {
				character.is_ascii_hexdigit()
			} else {
				character.is_ascii_digit()
			}
		});
		if !valid {
			return None;
		}

		// strconv.ParseInt(entity, base, 32): out of int32 range is an error.
		let parsed = i32::from_str_radix(digits, radix).ok()?;
		// Go's WriteRune writes U+FFFD for an invalid rune.
		return Some(
			u32::try_from(parsed)
				.ok()
				.and_then(char::from_u32)
				.unwrap_or('\u{FFFD}'),
		);
	}

	JSX_ENTITIES
		.iter()
		.find(|(name, _)| *name == entity)
		.map(|&(_, character)| character)
}

/// tsgo's `entities` table: the named HTML entity table from TS's
/// `fixupWhitespaceAndDecodeEntities.ts`.
const JSX_ENTITIES: &[(&str, char)] = &[
	("quot", '\u{0022}'),
	("amp", '\u{0026}'),
	("apos", '\u{0027}'),
	("lt", '\u{003C}'),
	("gt", '\u{003E}'),
	("nbsp", '\u{00A0}'),
	("iexcl", '\u{00A1}'),
	("cent", '\u{00A2}'),
	("pound", '\u{00A3}'),
	("curren", '\u{00A4}'),
	("yen", '\u{00A5}'),
	("brvbar", '\u{00A6}'),
	("sect", '\u{00A7}'),
	("uml", '\u{00A8}'),
	("copy", '\u{00A9}'),
	("ordf", '\u{00AA}'),
	("laquo", '\u{00AB}'),
	("not", '\u{00AC}'),
	("shy", '\u{00AD}'),
	("reg", '\u{00AE}'),
	("macr", '\u{00AF}'),
	("deg", '\u{00B0}'),
	("plusmn", '\u{00B1}'),
	("sup2", '\u{00B2}'),
	("sup3", '\u{00B3}'),
	("acute", '\u{00B4}'),
	("micro", '\u{00B5}'),
	("para", '\u{00B6}'),
	("middot", '\u{00B7}'),
	("cedil", '\u{00B8}'),
	("sup1", '\u{00B9}'),
	("ordm", '\u{00BA}'),
	("raquo", '\u{00BB}'),
	("frac14", '\u{00BC}'),
	("frac12", '\u{00BD}'),
	("frac34", '\u{00BE}'),
	("iquest", '\u{00BF}'),
	("Agrave", '\u{00C0}'),
	("Aacute", '\u{00C1}'),
	("Acirc", '\u{00C2}'),
	("Atilde", '\u{00C3}'),
	("Auml", '\u{00C4}'),
	("Aring", '\u{00C5}'),
	("AElig", '\u{00C6}'),
	("Ccedil", '\u{00C7}'),
	("Egrave", '\u{00C8}'),
	("Eacute", '\u{00C9}'),
	("Ecirc", '\u{00CA}'),
	("Euml", '\u{00CB}'),
	("Igrave", '\u{00CC}'),
	("Iacute", '\u{00CD}'),
	("Icirc", '\u{00CE}'),
	("Iuml", '\u{00CF}'),
	("ETH", '\u{00D0}'),
	("Ntilde", '\u{00D1}'),
	("Ograve", '\u{00D2}'),
	("Oacute", '\u{00D3}'),
	("Ocirc", '\u{00D4}'),
	("Otilde", '\u{00D5}'),
	("Ouml", '\u{00D6}'),
	("times", '\u{00D7}'),
	("Oslash", '\u{00D8}'),
	("Ugrave", '\u{00D9}'),
	("Uacute", '\u{00DA}'),
	("Ucirc", '\u{00DB}'),
	("Uuml", '\u{00DC}'),
	("Yacute", '\u{00DD}'),
	("THORN", '\u{00DE}'),
	("szlig", '\u{00DF}'),
	("agrave", '\u{00E0}'),
	("aacute", '\u{00E1}'),
	("acirc", '\u{00E2}'),
	("atilde", '\u{00E3}'),
	("auml", '\u{00E4}'),
	("aring", '\u{00E5}'),
	("aelig", '\u{00E6}'),
	("ccedil", '\u{00E7}'),
	("egrave", '\u{00E8}'),
	("eacute", '\u{00E9}'),
	("ecirc", '\u{00EA}'),
	("euml", '\u{00EB}'),
	("igrave", '\u{00EC}'),
	("iacute", '\u{00ED}'),
	("icirc", '\u{00EE}'),
	("iuml", '\u{00EF}'),
	("eth", '\u{00F0}'),
	("ntilde", '\u{00F1}'),
	("ograve", '\u{00F2}'),
	("oacute", '\u{00F3}'),
	("ocirc", '\u{00F4}'),
	("otilde", '\u{00F5}'),
	("ouml", '\u{00F6}'),
	("divide", '\u{00F7}'),
	("oslash", '\u{00F8}'),
	("ugrave", '\u{00F9}'),
	("uacute", '\u{00FA}'),
	("ucirc", '\u{00FB}'),
	("uuml", '\u{00FC}'),
	("yacute", '\u{00FD}'),
	("thorn", '\u{00FE}'),
	("yuml", '\u{00FF}'),
	("OElig", '\u{0152}'),
	("oelig", '\u{0153}'),
	("Scaron", '\u{0160}'),
	("scaron", '\u{0161}'),
	("Yuml", '\u{0178}'),
	("fnof", '\u{0192}'),
	("circ", '\u{02C6}'),
	("tilde", '\u{02DC}'),
	("Alpha", '\u{0391}'),
	("Beta", '\u{0392}'),
	("Gamma", '\u{0393}'),
	("Delta", '\u{0394}'),
	("Epsilon", '\u{0395}'),
	("Zeta", '\u{0396}'),
	("Eta", '\u{0397}'),
	("Theta", '\u{0398}'),
	("Iota", '\u{0399}'),
	("Kappa", '\u{039A}'),
	("Lambda", '\u{039B}'),
	("Mu", '\u{039C}'),
	("Nu", '\u{039D}'),
	("Xi", '\u{039E}'),
	("Omicron", '\u{039F}'),
	("Pi", '\u{03A0}'),
	("Rho", '\u{03A1}'),
	("Sigma", '\u{03A3}'),
	("Tau", '\u{03A4}'),
	("Upsilon", '\u{03A5}'),
	("Phi", '\u{03A6}'),
	("Chi", '\u{03A7}'),
	("Psi", '\u{03A8}'),
	("Omega", '\u{03A9}'),
	("alpha", '\u{03B1}'),
	("beta", '\u{03B2}'),
	("gamma", '\u{03B3}'),
	("delta", '\u{03B4}'),
	("epsilon", '\u{03B5}'),
	("zeta", '\u{03B6}'),
	("eta", '\u{03B7}'),
	("theta", '\u{03B8}'),
	("iota", '\u{03B9}'),
	("kappa", '\u{03BA}'),
	("lambda", '\u{03BB}'),
	("mu", '\u{03BC}'),
	("nu", '\u{03BD}'),
	("xi", '\u{03BE}'),
	("omicron", '\u{03BF}'),
	("pi", '\u{03C0}'),
	("rho", '\u{03C1}'),
	("sigmaf", '\u{03C2}'),
	("sigma", '\u{03C3}'),
	("tau", '\u{03C4}'),
	("upsilon", '\u{03C5}'),
	("phi", '\u{03C6}'),
	("chi", '\u{03C7}'),
	("psi", '\u{03C8}'),
	("omega", '\u{03C9}'),
	("thetasym", '\u{03D1}'),
	("upsih", '\u{03D2}'),
	("piv", '\u{03D6}'),
	("ensp", '\u{2002}'),
	("emsp", '\u{2003}'),
	("thinsp", '\u{2009}'),
	("zwnj", '\u{200C}'),
	("zwj", '\u{200D}'),
	("lrm", '\u{200E}'),
	("rlm", '\u{200F}'),
	("ndash", '\u{2013}'),
	("mdash", '\u{2014}'),
	("lsquo", '\u{2018}'),
	("rsquo", '\u{2019}'),
	("sbquo", '\u{201A}'),
	("ldquo", '\u{201C}'),
	("rdquo", '\u{201D}'),
	("bdquo", '\u{201E}'),
	("dagger", '\u{2020}'),
	("Dagger", '\u{2021}'),
	("bull", '\u{2022}'),
	("hellip", '\u{2026}'),
	("permil", '\u{2030}'),
	("prime", '\u{2032}'),
	("Prime", '\u{2033}'),
	("lsaquo", '\u{2039}'),
	("rsaquo", '\u{203A}'),
	("oline", '\u{203E}'),
	("frasl", '\u{2044}'),
	("euro", '\u{20AC}'),
	("image", '\u{2111}'),
	("weierp", '\u{2118}'),
	("real", '\u{211C}'),
	("trade", '\u{2122}'),
	("alefsym", '\u{2135}'),
	("larr", '\u{2190}'),
	("uarr", '\u{2191}'),
	("rarr", '\u{2192}'),
	("darr", '\u{2193}'),
	("harr", '\u{2194}'),
	("crarr", '\u{21B5}'),
	("lArr", '\u{21D0}'),
	("uArr", '\u{21D1}'),
	("rArr", '\u{21D2}'),
	("dArr", '\u{21D3}'),
	("hArr", '\u{21D4}'),
	("forall", '\u{2200}'),
	("part", '\u{2202}'),
	("exist", '\u{2203}'),
	("empty", '\u{2205}'),
	("nabla", '\u{2207}'),
	("isin", '\u{2208}'),
	("notin", '\u{2209}'),
	("ni", '\u{220B}'),
	("prod", '\u{220F}'),
	("sum", '\u{2211}'),
	("minus", '\u{2212}'),
	("lowast", '\u{2217}'),
	("radic", '\u{221A}'),
	("prop", '\u{221D}'),
	("infin", '\u{221E}'),
	("ang", '\u{2220}'),
	("and", '\u{2227}'),
	("or", '\u{2228}'),
	("cap", '\u{2229}'),
	("cup", '\u{222A}'),
	("int", '\u{222B}'),
	("there4", '\u{2234}'),
	("sim", '\u{223C}'),
	("cong", '\u{2245}'),
	("asymp", '\u{2248}'),
	("ne", '\u{2260}'),
	("equiv", '\u{2261}'),
	("le", '\u{2264}'),
	("ge", '\u{2265}'),
	("sub", '\u{2282}'),
	("sup", '\u{2283}'),
	("nsub", '\u{2284}'),
	("sube", '\u{2286}'),
	("supe", '\u{2287}'),
	("oplus", '\u{2295}'),
	("otimes", '\u{2297}'),
	("perp", '\u{22A5}'),
	("sdot", '\u{22C5}'),
	("lceil", '\u{2308}'),
	("rceil", '\u{2309}'),
	("lfloor", '\u{230A}'),
	("rfloor", '\u{230B}'),
	("lang", '\u{2329}'),
	("rang", '\u{232A}'),
	("loz", '\u{25CA}'),
	("spades", '\u{2660}'),
	("clubs", '\u{2663}'),
	("hearts", '\u{2665}'),
	("diams", '\u{2666}'),
];
