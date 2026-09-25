# Fork-Parity Matrix Data

`divergence-ledger.json` is the machine-readable authority map for the zip
compatibility matrix. It contains no hand-authored compiler output. The frozen
archive-captured transformer bytes, project trees, and transcripts live under
`transformer/` and `project/`; each corpus has a `provenance.json` with the
archive digest and capture command. A full matrix run writes its normalized
zero-drift report to the Go test artifact directory.

`upstream-corrected` rows preserve those frozen bytes as historical evidence.
They accept byte differences only after the named behavioral test passes against
[the pinned upstream compiler](https://github.com/roblox-ts/roblox-ts/tree/3106b1492a2cf5b5b06f73354e94b03e3eb45b4e)
and Rotor. The report records accepted differences in `archiveDifferences`;
compiler errors and other unexpected differences still fail. Missing or skipped
runtime tests cannot verify a correction.
