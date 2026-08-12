# Native and upstream Flamework 1.3.2 differential corpus

The Go differential test copies `project/` twice, installs the exact dependencies
from its pinned `package.json`, and runs the real
`rbxts-transformer-flamework@1.3.2` through `rbxtsc` in one copy. It runs Rotor's
native `Transform` plus Rotor's Luau lowerer in the other copy.

`deterministic-random.cjs` fixes the upstream UUID generators and `Math.random`.
The native run injects the same UUID and random-index results. Every option case
also fixes `salt`, `hashPrefix`, package identity, file order, and the guard
deduplication limit. Final Luau is compared byte-for-byte. Diagnostics are
compared as ordered file/position/message tuples. JSON artifacts are decoded and
re-encoded before comparison because upstream and Rotor intentionally use
different insignificant indentation and object-key order. Glob match/origin
arrays are sorted during that structural comparison: upstream records filesystem
traversal order while Rotor deliberately emits the same path set in stable sorted
order. No Luau, diagnostic, identifier, hash, UUID, or scalar artifact value is
normalized.

`coverage.tsv` is the machine-consumed case-to-family matrix.
`upstream-inventory.tsv` pins every v1.3.2 source entry; the test rejects any
unaccounted entry or behavior family.
