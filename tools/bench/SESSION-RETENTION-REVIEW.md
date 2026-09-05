# Synthetic session-retention review

This is a component measurement of the production `SidecarServer` and its
session state. It creates one synthetic TypeScript source, adds and deletes a
different synthetic source 300 times, and samples metadata after every 50
cycles. The raw samples are in
[`synthetic-worker-results/session-retention-soak.json`](synthetic-worker-results/session-retention-soak.json).

The historical baseline is immutable commit
`5ce06e9e07914e56cb7f727a403b3994d6bf56f8` using protocol 2. The candidate
uses the current production `SidecarServer` with protocol 3 and a captured
config snapshot. Both requests include their project path, config path, full
source list, roots, changed files, and content identities, so the server's
normal protocol validation runs.

| Metric after 300 cycles | Baseline | Candidate |
| --- | ---: | ---: |
| `actualPaths` | 602 | 2 |
| `pathAliases` | 303 | 4 |
| `versions` | 602 | 2 |
| `deleted` | 300 | 0 |
| `baseRoots` | 1 | 1 |
| `rootLimit` | 301 | 1 |

Candidate metadata has the same values at cycles 0, 50, 100, 150, 200, 250,
and 300. The baseline grows on every interval. This demonstrates that the
deletion cleanup bounds the session metadata tracked by this fixture.

Run the two isolated measurements from the repository root with the pinned
runtime:

```sh
SESSION_RETENTION_SOAK_MODE=baseline mise x -- node tools/bench/session-retention-soak.cjs
SESSION_RETENTION_SOAK_MODE=candidate mise x -- node tools/bench/session-retention-soak.cjs
```

The recorded run used Node `v22.23.2` on macOS arm64. Its baseline session
source SHA-256 is
`d0343863bbc8b7dbc064098896d6b762d1255624f50c2f00b28b92b6bb90abef`; the
candidate SHA-256 is
`80a9218fbad508691f64b355d04b9b6fcc317915a9a615cdbbab201a098c56f6`.
The script emits the source hash for every run. Compare it before treating a
new candidate run as the same cohort.

The result does not measure CLI work, IPC transport, daemon startup, plugin
factories or visitors, Go process memory, or a complete worker's residency.
It does not force garbage collection. RSS and V8 heap values are retained as
observations in the raw data, but they do not establish a memory leak or a
whole-worker memory plateau. The only acceptance claim here is bounded session
metadata for this synthetic add/delete workload.
