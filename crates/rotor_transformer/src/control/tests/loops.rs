//! The closure cases of `loops_test.go`, which compile function expressions. The expected text
//! is byte for byte what rbxtsc 3.0.0 emits for the same source.

use super::render_file;

/// a closure capturing the loop variable does NOT disqualify the optimized numeric-for pass (its
/// loop variable is per-iteration in Luau already), so no copy machinery appears.
#[test]
fn optimized_loop_closure_no_copies() {
	let want = "local fns = {}
for i = 0, 2 do
	fns[i + 1] = function()
		return i
	end
end
print(fns[1]())
";
	assert_eq!(render_file("control", "src/optclosure.ts"), want);
}

/// condition `i !== 3` forces the fallback, and the closure read of `i` (the "async read")
/// triggers the per-iteration copy machinery — outer `local _i = 0` slot, `local i = _i` body-head
/// copy, and the `_i = i` finalizer write-back. This input panicked in Phase 2.
#[test]
fn loop_closure_capture_copies() {
	let want = "do
	local _i = 0
	local _shouldIncrement = false
	while true do
		local i = _i
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 3) then
			break
		end
		local f = function()
			return i
		end
		print(f())
		_i = i
	end
end
";
	assert_eq!(render_file("control", "src/capture.ts"), want);
}
