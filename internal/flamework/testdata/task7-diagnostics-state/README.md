# Task 7 diagnostic-state fixture

This project is compiled by both the pinned `rbxts-transformer-flamework` 1.3.2
entrypoint and Rotor's exported native Flamework APIs. Tests copy it to fresh
directories before applying one malformed-state mutation, so state cannot leak
between diagnostic rows.
