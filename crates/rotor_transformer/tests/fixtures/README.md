# Differential fixture lists

Each `*.txt` file here lists `testdata/diff/golden` fixtures, one name per line, that `tests/diff.rs`
compiles and requires to be byte-identical to the golden. `core.txt` is Stage 4a's gate; each
Stage 4b slice owns its own `slice_*.txt` (see `SLICES.md`), so slices never edit the same list.
Blank lines and `#` comments are ignored.
