//! `try_exit_test.go`: the guarded and unguarded post-try dispatch.

use super::render_origin_source;

#[test]
fn try_always_exits_return() {
	let got = render_origin_source(
		"
declare function load(): number;
declare function report(error: unknown): void;
function read(): number {
	try { return load(); }
	catch (error) { report(error); return 0; }
}",
	);
	let want = "local function read()
	local _exitType, _returns = TS.try(function()
		return TS.TRY_RETURN, { load() }
	end, function(error)
		report(error)
		return TS.TRY_RETURN, { 0 }
	end)
	return unpack(_returns)
end
";
	assert_eq!(got, want, "output differs");
}

struct ExitCase {
	name: &'static str,
	source: &'static str,
	guarded: bool,
	want: &'static str,
}

#[test]
#[expect(clippy::too_many_lines, reason = "the case table is verbatim")]
fn try_exit_dispatch() {
	let tests = [
		ExitCase {
			name: "catch falls through",
			source: "function f(): number {
				try { return 1; } catch { }
				return -1;
			}",
			guarded: true,
			want: "\tif _exitType then\n\t\treturn unpack(_returns)\n\tend\n\treturn -1\n",
		},
		ExitCase {
			name: "try falls through",
			source: "function f(flag: boolean) {
				try { if (flag) return 1; } catch { return 0; }
			}",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "both if branches exit",
			source: r#"function f(flag: boolean) {
				try { if (flag) { return 1; } else { throw "error"; } }
				catch { return 0; }
			}"#,
			guarded: false,
			want: "\treturn unpack(_returns)\nend\n",
		},
		ExitCase {
			name: "nested block exits",
			source: r#"function f() {
				try { { return 1; } } catch { throw "error"; }
			}"#,
			guarded: false,
			want: "",
		},
		ExitCase {
			name: "normal finally preserves exit",
			source: "declare function cleanup(): void;
			function f() {
				try { return 1; } catch { return 0; } finally { cleanup(); }
			}",
			guarded: false,
			want: "\treturn unpack(_returns)\nend\n",
		},
		ExitCase {
			name: "no catch",
			source: "function f() { try { return 1; } finally {} }",
			guarded: false,
			want: "",
		},
		ExitCase {
			name: "finally alone does not prove exit",
			source: "function f() { try {} finally { return 1; } }",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "never call can return in try",
			source: "declare function fail(): never;
			function f() { try { fail(); } catch { return 0; } }",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "never call can return in catch",
			source: "declare function fail(): never;
			function f() { try { return 1; } catch { fail(); } }",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "inner function return is not an exit",
			source: "function f() {
				try { const callback = () => { return 1; }; callback(); }
				catch { return 0; }
			}",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "inner loop break is not an exit",
			source: "function f() { try { while (true) { break; } } catch { return 0; } }",
			guarded: true,
			want: "",
		},
		ExitCase {
			name: "unreachable statements preserve guard",
			source: "declare function after(): void;
			function f() { try { return 1; } catch { return 0; } after(); }",
			guarded: true,
			want: "\tif _exitType then\n\t\treturn unpack(_returns)\n\tend\n\tafter()\n",
		},
		ExitCase {
			name: "unbraced if body",
			source: "function f(flag: boolean) { if (flag) try { return 1; } catch { return 0; } }",
			guarded: false,
			want: "",
		},
		ExitCase {
			name: "unbraced loop body",
			source: "function f() { while (true) try { break; } catch { break; } }",
			guarded: false,
			want: "\t\tbreak\n\tend\n",
		},
		ExitCase {
			name: "continue only",
			source: "function f() { while (true) { try { continue; } catch { continue; } } }",
			guarded: false,
			want: "\t\tcontinue\n\tend\n",
		},
		ExitCase {
			name: "direct labeled try",
			source: "function f() { done: try { return 1; } catch { return 0; } }",
			guarded: false,
			want: "\t\treturn unpack(_returns)\n\tuntil true\n",
		},
		ExitCase {
			name: "switch clause tail",
			source: "function f(value: number) {
				switch (value) { case 1: try { return 1; } catch { return 0; } }
			}",
			guarded: false,
			want: "",
		},
		ExitCase {
			name: "nested return tunnels through enclosing try",
			source: "function f() {
				try { try { return 1; } catch { return 0; } } catch {}
			}",
			guarded: true,
			want: "\t\treturn _exitType_1, _returns_1\n\tend, function() end)\n\tif _exitType then",
		},
		ExitCase {
			name: "mixed loop exits",
			source: "function f(flag: boolean) {
				while (true) {
					try { if (flag) { break; } else { return 1; } }
					catch { continue; }
				}
			}",
			guarded: false,
			want: "\t\tif _exitType == TS.TRY_RETURN then\n\t\t\treturn unpack(_returns)\n\t\telseif _exitType == TS.TRY_BREAK then\n\t\t\tbreak\n\t\telse\n\t\t\tcontinue\n\t\tend\n",
		},
	];
	for test in &tests {
		let got = render_origin_source(test.source);
		let has_guard = got.contains("if _exitType then");
		assert_eq!(
			has_guard, test.guarded,
			"{}: exit guard = {has_guard}, want {}:\n{got}",
			test.name, test.guarded
		);
		assert!(
			test.want.is_empty() || got.contains(test.want),
			"{}: missing expected output {:?}:\n{got}",
			test.name,
			test.want
		);
	}
}
