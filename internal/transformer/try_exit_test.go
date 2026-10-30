package transformer

import (
	"strings"
	"testing"

	"rotor/internal/luau/render"
)

func TestTryAlwaysExitsReturn(t *testing.T) {
	got := render.RenderAST(transformOriginSource(t, `
declare function load(): number;
declare function report(error: unknown): void;
function read(): number {
	try { return load(); }
	catch (error) { report(error); return 0; }
}`))
	want := `local function read()
	local _exitType, _returns = TS.try(function()
		return TS.TRY_RETURN, { load() }
	end, function(error)
		report(error)
		return TS.TRY_RETURN, { 0 }
	end)
	return unpack(_returns)
end
`
	if got != want {
		t.Fatalf("output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestTryExitDispatch(t *testing.T) {
	tests := []struct {
		name    string
		source  string
		guarded bool
		want    string
	}{
		{
			name: "catch falls through",
			source: `function f(): number {
				try { return 1; } catch { }
				return -1;
			}`,
			guarded: true,
			want:    "\tif _exitType then\n\t\treturn unpack(_returns)\n\tend\n\treturn -1\n",
		},
		{
			name: "try falls through",
			source: `function f(flag: boolean) {
				try { if (flag) return 1; } catch { return 0; }
			}`,
			guarded: true,
		},
		{
			name: "both if branches exit",
			source: `function f(flag: boolean) {
				try { if (flag) { return 1; } else { throw "error"; } }
				catch { return 0; }
			}`,
			want: "\treturn unpack(_returns)\nend\n",
		},
		{
			name: "nested block exits",
			source: `function f() {
				try { { return 1; } } catch { throw "error"; }
			}`,
		},
		{
			name: "normal finally preserves exit",
			source: `declare function cleanup(): void;
			function f() {
				try { return 1; } catch { return 0; } finally { cleanup(); }
			}`,
			want: "\treturn unpack(_returns)\nend\n",
		},
		{
			name:   "no catch",
			source: `function f() { try { return 1; } finally {} }`,
		},
		{
			name:    "finally alone does not prove exit",
			source:  `function f() { try {} finally { return 1; } }`,
			guarded: true,
		},
		{
			name: "never call can return in try",
			source: `declare function fail(): never;
			function f() { try { fail(); } catch { return 0; } }`,
			guarded: true,
		},
		{
			name: "never call can return in catch",
			source: `declare function fail(): never;
			function f() { try { return 1; } catch { fail(); } }`,
			guarded: true,
		},
		{
			name: "inner function return is not an exit",
			source: `function f() {
				try { const callback = () => { return 1; }; callback(); }
				catch { return 0; }
			}`,
			guarded: true,
		},
		{
			name:    "inner loop break is not an exit",
			source:  `function f() { try { while (true) { break; } } catch { return 0; } }`,
			guarded: true,
		},
		{
			name: "unreachable statements preserve guard",
			source: `declare function after(): void;
			function f() { try { return 1; } catch { return 0; } after(); }`,
			guarded: true,
			want:    "\tif _exitType then\n\t\treturn unpack(_returns)\n\tend\n\tafter()\n",
		},
		{
			name:   "unbraced if body",
			source: `function f(flag: boolean) { if (flag) try { return 1; } catch { return 0; } }`,
		},
		{
			name:   "unbraced loop body",
			source: `function f() { while (true) try { break; } catch { break; } }`,
			want:   "\t\tbreak\n\tend\n",
		},
		{
			name:   "continue only",
			source: `function f() { while (true) { try { continue; } catch { continue; } } }`,
			want:   "\t\tcontinue\n\tend\n",
		},
		{
			name: "switch clause tail",
			source: `function f(value: number) {
				switch (value) { case 1: try { return 1; } catch { return 0; } }
			}`,
		},
		{
			name: "nested return tunnels through enclosing try",
			source: `function f() {
				try { try { return 1; } catch { return 0; } } catch {}
			}`,
			guarded: true,
			want:    "\t\treturn _exitType_1, _returns_1\n\tend, function() end)\n\tif _exitType then",
		},
		{
			name: "mixed loop exits",
			source: `function f(flag: boolean) {
				while (true) {
					try { if (flag) { break; } else { return 1; } }
					catch { continue; }
				}
			}`,
			want: "\t\tif _exitType == TS.TRY_RETURN then\n\t\t\treturn unpack(_returns)\n\t\telseif _exitType == TS.TRY_BREAK then\n\t\t\tbreak\n\t\telse\n\t\t\tcontinue\n\t\tend\n",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := render.RenderAST(transformOriginSource(t, test.source))
			hasGuard := strings.Contains(got, "if _exitType then")
			if hasGuard != test.guarded {
				t.Fatalf("exit guard = %v, want %v:\n%s", hasGuard, test.guarded, got)
			}
			if test.want != "" && !strings.Contains(got, test.want) {
				t.Fatalf("missing expected output %q:\n%s", test.want, got)
			}
		})
	}
}
