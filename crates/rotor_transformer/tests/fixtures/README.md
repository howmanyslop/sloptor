# Differential fixture lists

Each `*.txt` file here lists `testdata/diff/golden` fixtures, one name per line, that `tests/diff.rs`
compiles and requires to be byte-identical to the golden. `core.txt` lists the core's fixtures and
each `slice_*.txt` those of one module (see `SLICES.md`). Together they cover every fixture except the
upstream-corrected ledger rows.
Blank lines and `#` comments are ignored.
