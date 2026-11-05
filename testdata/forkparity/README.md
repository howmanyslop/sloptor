# Fork-Parity Matrix Data

`divergence-ledger.json` is the machine-readable authority map for the zip
compatibility matrix. It contains no hand-authored compiler output. The frozen
archive-captured transformer bytes, project trees, and transcripts live under
`transformer/` and `project/`; each corpus has a `provenance.json` with the
archive digest and capture command. `mise run rust:forkparity` runs the matrix
(`crates/rotor_cli/tests/forkparity.rs`) against the sloptor release binary.

`upstream-corrected` rows preserve those frozen bytes as historical evidence.
They accept byte differences only after the named behavioral test (a
`rotor_cli::forkparity::runtime::*` test) passes against
[the pinned upstream compiler](https://github.com/roblox-ts/roblox-ts/tree/3106b1492a2cf5b5b06f73354e94b03e3eb45b4e)
and sloptor. The run prints the accepted differences; compiler errors and other
unexpected differences still fail. Missing or skipped
runtime tests cannot verify a correction.
