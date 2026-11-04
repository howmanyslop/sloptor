//! `tsgo/jsonrpc`'s base protocol: messages framed by a `Content-Length` header.

use std::io::{BufRead, BufReader, Read, Write};

/// Why a frame could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
	/// The stream ended between frames.
	Eof,
	/// Anything else, with Go's text.
	Failed(String),
}

impl std::fmt::Display for ReadError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Eof => formatter.write_str("EOF"),
			Self::Failed(message) => formatter.write_str(message),
		}
	}
}

/// `Reader`
pub struct Reader<R> {
	input: BufReader<R>,
}

impl<R: Read> Reader<R> {
	/// `NewReader`
	pub fn new(input: R) -> Self {
		Self {
			input: BufReader::new(input),
		}
	}

	/// `Read`: the next frame's payload.
	///
	/// # Errors
	///
	/// [`ReadError::Eof`] at the end of the stream, or a malformed header or body.
	pub fn read(&mut self) -> Result<Vec<u8>, ReadError> {
		let mut content_length: i64 = 0;
		loop {
			let mut line = Vec::new();
			match self.input.read_until(b'\n', &mut line) {
				Ok(0) => return Err(ReadError::Eof),
				Ok(_) if !line.ends_with(b"\n") => return Err(ReadError::Eof),
				Ok(_) => {}
				Err(error) => return Err(ReadError::Failed(format!("jsonrpc: read header: {error}"))),
			}
			if line == b"\r\n" {
				break;
			}
			let Some(colon) = line.iter().position(|&byte| byte == b':') else {
				return Err(ReadError::Failed(format!(
					"jsonrpc: invalid header: {:?}",
					String::from_utf8_lossy(&line)
				)));
			};
			if &line[..colon] == b"Content-Length" {
				let value = String::from_utf8_lossy(&line[colon + 1..]).trim().to_owned();
				content_length = value.parse().map_err(|error| {
					ReadError::Failed(format!(
						"jsonrpc: invalid content length: parse error: strconv.ParseInt: parsing {value:?}: {error}"
					))
				})?;
				if content_length < 0 {
					return Err(ReadError::Failed(format!(
						"jsonrpc: invalid content length: negative value {content_length}"
					)));
				}
			}
		}
		if content_length <= 0 {
			return Err(ReadError::Failed("jsonrpc: no content length".to_owned()));
		}
		let mut data = vec![0; usize::try_from(content_length).unwrap_or(0)];
		self.input
			.read_exact(&mut data)
			.map_err(|error| ReadError::Failed(format!("jsonrpc: read content: {error}")))?;
		Ok(data)
	}
}

/// `Writer.Write`: one frame, header and payload, flushed.
///
/// # Errors
///
/// The stream refused the write.
pub fn write(output: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
	let mut frame = format!("Content-Length: {}\r\n\r\n", data.len()).into_bytes();
	frame.extend_from_slice(data);
	output.write_all(&frame)?;
	output.flush()
}

#[cfg(test)]
mod tests {
	use super::{ReadError, Reader, write};

	#[test]
	fn frames_round_trip() {
		let mut out = Vec::new();
		write(&mut out, b"{\"a\":1}").expect("write");
		write(&mut out, b"[]").expect("write");
		assert_eq!(out, b"Content-Length: 7\r\n\r\n{\"a\":1}Content-Length: 2\r\n\r\n[]");
		let mut reader = Reader::new(&out[..]);
		assert_eq!(reader.read().expect("first"), b"{\"a\":1}");
		assert_eq!(reader.read().expect("second"), b"[]");
		assert_eq!(reader.read(), Err(ReadError::Eof));
	}

	#[test]
	fn malformed_headers_are_errors() {
		let mut reader = Reader::new(&b"garbage\r\n\r\n"[..]);
		assert!(
			matches!(reader.read(), Err(ReadError::Failed(message)) if message.starts_with("jsonrpc: invalid header"))
		);
		let mut reader = Reader::new(&b"Content-Type: x\r\n\r\n"[..]);
		assert_eq!(
			reader.read(),
			Err(ReadError::Failed("jsonrpc: no content length".to_owned()))
		);
	}
}
