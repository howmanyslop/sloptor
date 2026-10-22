package render

import (
	"math/rand"
	"strconv"
	"testing"

	"rotor/internal/luau"
)

func solveFor(stmts *luau.List[luau.Statement]) *RenderState {
	s := NewRenderState()
	solveTempIDs(s, stmts)
	return s
}

func TestTempIdsBasic(t *testing.T) {
	t1, t2 := luau.TempID(""), luau.TempID("")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(t1, luau.Num(1)),
		luau.NewVariableDeclaration(t2, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[t1.ID]; got != "_" {
		t.Errorf("first temp = %q, want %q", got, "_")
	}
	if got := s.seenTempNodes[t2.ID]; got != "_1" {
		t.Errorf("second temp = %q, want %q", got, "_1")
	}
}

func TestTempIdsNamed(t *testing.T) {
	t1, t2 := luau.TempID("foo"), luau.TempID("foo")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(t1, luau.Num(1)),
		luau.NewVariableDeclaration(t2, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[t1.ID]; got != "_foo" {
		t.Errorf("first = %q, want _foo", got)
	}
	if got := s.seenTempNodes[t2.ID]; got != "_foo_1" {
		t.Errorf("second = %q, want _foo_1", got)
	}
}

func TestTempIdsExactNamed(t *testing.T) {
	tmp := luau.ExactTempID("foo")
	statements := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(tmp, luau.Num(1)),
	)
	state := solveFor(statements)
	if got := state.seenTempNodes[tmp.ID]; got != "foo" {
		t.Errorf("exact temp = %q, want foo", got)
	}
}

func TestTempIdsExactNamedAvoidCollision(t *testing.T) {
	tmp := luau.ExactTempID("foo")
	statements := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(luau.ID("foo"), luau.Num(1)),
		luau.NewVariableDeclaration(tmp, luau.Num(2)),
	)
	state := solveFor(statements)
	if got := state.seenTempNodes[tmp.ID]; got != "foo_1" {
		t.Errorf("exact temp = %q, want foo_1", got)
	}
}

func TestTempIdsAvoidDeclaredLocals(t *testing.T) {
	tmp := luau.TempID("foo")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(luau.ID("_foo"), luau.Num(1)),
		luau.NewVariableDeclaration(tmp, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[tmp.ID]; got != "_foo_1" {
		t.Errorf("temp = %q, want _foo_1 (collides with declared _foo)", got)
	}
}

func TestTempIdsAvoidLocalFunctionDeclarations(t *testing.T) {
	tmp := luau.TempID("foo")
	statements := luau.NewList[luau.Statement](
		luau.NewFunctionDeclaration(
			true,
			luau.ID("_foo"),
			luau.NewList[luau.AnyIdentifier](),
			false,
			luau.NewList[luau.Statement](),
		),
		luau.NewFunctionDeclaration(
			true,
			tmp,
			luau.NewList[luau.AnyIdentifier](),
			false,
			luau.NewList[luau.Statement](),
		),
	)
	state := solveFor(statements)
	if got := state.seenTempNodes[tmp.ID]; got != "_foo_1" {
		t.Errorf("temp = %q, want _foo_1 (collides with local function _foo)", got)
	}
}

func TestTempIdsAvoidIdentifierUses(t *testing.T) {
	tmp := luau.TempID("foo")
	statements := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(tmp, luau.Num(1)),
		luau.NewCallStatement(luau.NewCall(
			luau.ID("_foo"),
			luau.NewList[luau.Expression](),
		)),
	)
	state := solveFor(statements)
	if got := state.seenTempNodes[tmp.ID]; got != "_foo_1" {
		t.Errorf("temp = %q, want _foo_1 (collides with identifier use _foo)", got)
	}
}

func TestTempIdsAvoidNestedIdentifierUses(t *testing.T) {
	tmp := luau.TempID("foo")
	statements := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(tmp, luau.Num(1)),
		luau.NewFunctionDeclaration(
			true,
			luau.ID("later"),
			luau.NewList[luau.AnyIdentifier](),
			false,
			luau.NewList[luau.Statement](luau.NewCallStatement(luau.NewCall(
				luau.ID("_foo"),
				luau.NewList[luau.Expression](),
			))),
		),
	)
	state := solveFor(statements)
	if got := state.seenTempNodes[tmp.ID]; got != "_foo_1" {
		t.Errorf("root temp = %q, want _foo_1 (collides with nested identifier use _foo)", got)
	}
}

func TestTempIdsScopedFunctionsDontCollide(t *testing.T) {
	// two function expressions each with their own temp: both may use "_"
	mk := func() (*luau.TemporaryIdentifier, luau.Statement) {
		tmp := luau.TempID("")
		body := luau.NewList[luau.Statement](luau.NewVariableDeclaration(tmp, luau.Num(1)))
		fn := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, body)
		return tmp, luau.NewVariableDeclaration(luau.ID("f"), fn)
	}
	ta, sa := mk()
	tb, sb := mk()
	s := solveFor(luau.NewList[luau.Statement](sa, sb))
	if s.seenTempNodes[ta.ID] != "_" || s.seenTempNodes[tb.ID] != "_" {
		t.Errorf("separate function scopes should both get %q: got %q, %q",
			"_", s.seenTempNodes[ta.ID], s.seenTempNodes[tb.ID])
	}
}

func TestTempIdsOverlappingHints(t *testing.T) {
	cases := []struct {
		name  string
		temps []*luau.TemporaryIdentifier
		want  []string
	}{
		{"numbered hints", []*luau.TemporaryIdentifier{luau.TempID("foo"), luau.TempID("foo"), luau.TempID("foo_1")}, []string{"_foo", "_foo_1", "_foo_1_2"}},
		{"normal and exact", []*luau.TemporaryIdentifier{luau.TempID("foo"), luau.TempID("foo"), luau.ExactTempID("_foo_1")}, []string{"_foo", "_foo_1", "_foo_1_2"}},
		{"anonymous and exact", []*luau.TemporaryIdentifier{luau.TempID(""), luau.TempID(""), luau.ExactTempID("_1")}, []string{"_", "_1", "_1_2"}},
		{"different separators", []*luau.TemporaryIdentifier{luau.TempID(""), luau.ExactTempID("_"), luau.TempID(""), luau.ExactTempID("_")}, []string{"_", "__1", "_1", "__2"}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			statements := luau.NewList[luau.Statement]()
			want := ""
			for i, tmp := range tc.temps {
				statements.Push(luau.NewVariableDeclaration(tmp, luau.Num(1)))
				// Reusing a temp must keep its first assigned name.
				statements.Push(luau.NewCallStatement(luau.NewCall(tmp, luau.NewList[luau.Expression]())))
				want += "local " + tc.want[i] + " = 1\n" + tc.want[i] + "()\n"
			}
			if got := renderAST(t, statements); got != want {
				t.Fatalf("rendered names:\n%s\nwant:\n%s", got, want)
			}
		})
	}
}

func TestTempIdsAncestorAndSiblingReservations(t *testing.T) {
	declaration := func() luau.Statement {
		return luau.NewVariableDeclaration(luau.TempID("foo"), luau.Num(1))
	}
	child := func() luau.Statement {
		return luau.NewDo(luau.NewList[luau.Statement](declaration(), declaration()))
	}
	statements := luau.NewList[luau.Statement](
		declaration(), child(), declaration(), child(),
		// This late declaration must reserve the ancestor name in both children.
		luau.NewVariableDeclaration(luau.ID("_foo_2"), luau.Num(0)),
	)
	want := "local _foo = 1\ndo\n\tlocal _foo_1 = 1\n\tlocal _foo_3 = 1\nend\n" +
		"local _foo_1 = 1\ndo\n\tlocal _foo_3 = 1\n\tlocal _foo_4 = 1\nend\nlocal _foo_2 = 0\n"
	if got := renderAST(t, statements); got != want {
		t.Fatalf("rendered names:\n%s\nwant:\n%s", got, want)
	}
}

// legacyTempNames is the original probing rule, independent of the optimized
// solver's scope walk and progress state. Final-name counter writes are part of
// the output contract: a later overlapping hint can read them.
type legacyTempNames struct {
	ids     map[string]bool
	lastTry map[string]int
	parent  *legacyTempNames
}

func (s *legacyTempNames) has(name string) bool {
	return s.ids[name] || s.parent != nil && s.parent.has(name)
}

func (s *legacyTempNames) name(tmp *luau.TemporaryIdentifier) string {
	base, separator := tmp.Name, "_"
	if !tmp.ExactName {
		base = "_" + base
		if tmp.Name == "" {
			separator = ""
		}
	}
	input, i := base, s.lastTry[base]
	if i == 0 {
		i = 1
	}
	for s.has(input) {
		input = base + separator + strconv.Itoa(i)
		i++
	}
	s.ids[input], s.lastTry[input] = true, i
	return input
}

func TestTempIdsMixedHintsMatchLegacy(t *testing.T) {
	rng := rand.New(rand.NewSource(1337))
	hints := []string{"", "foo", "foo_1", "foo_2", "_foo", "_foo_1", "_", "_1", "_1_2"}
	for scenario := 0; scenario < 25; scenario++ {
		type pendingTemp struct {
			tmp   *luau.TemporaryIdentifier
			scope *legacyTempNames
		}
		var pending []pendingTemp
		var build func(int, *legacyTempNames) *luau.List[luau.Statement]
		build = func(depth int, parent *legacyTempNames) *luau.List[luau.Statement] {
			scope := &legacyTempNames{ids: map[string]bool{}, lastTry: map[string]int{}, parent: parent}
			statements := luau.NewList[luau.Statement]()
			for i := 0; i < 16; i++ {
				if i%4 == 0 {
					// Declarations reserve names before solving even when they
					// occur after a child or leave holes in the suffix sequence.
					name := "_" + hints[rng.Intn(len(hints))] + []string{"", "_1", "_3", "_7"}[rng.Intn(4)]
					scope.ids[name] = true
					statements.Push(luau.NewVariableDeclaration(luau.ID(name), luau.Num(0)))
				}
				tmp := luau.TempID(hints[rng.Intn(len(hints))])
				if rng.Intn(2) == 0 && tmp.Name != "" {
					tmp.ExactName = true
				}
				pending = append(pending, pendingTemp{tmp, scope})
				statements.Push(luau.NewVariableDeclaration(tmp, luau.Num(1)))
				if depth < 2 && (i == 5 || i == 10) {
					statements.Push(luau.NewDo(build(depth+1, scope)))
				}
			}
			return statements
		}
		statements := build(0, nil)
		oracle := NewRenderState()
		for _, entry := range pending {
			oracle.seenTempNodes[entry.tmp.ID] = entry.scope.name(entry.tmp)
		}
		if got, want := renderAST(t, statements), renderStatements(oracle, statements); got != want {
			t.Fatalf("scenario %d differs from legacy naming:\n%s\nwant:\n%s", scenario, got, want)
		}
	}
}

func TestTempIdsRepeatedHintsAllocations(t *testing.T) {
	statements := repeatedTempStatements(1000)
	// A thousand same-hint temps previously allocated about 900,000 objects
	// by repeatedly formatting occupied suffixes. Allow ample renderer overhead
	// while ensuring that collision probing stays within a linear budget.
	if allocations := testing.AllocsPerRun(1, func() { RenderAST(statements) }); allocations > 20000 {
		t.Fatalf("1000 repeated hints allocated %.0f objects, want at most 20000", allocations)
	}
}

func repeatedTempStatements(count int) *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	for i := 0; i < count; i++ {
		statements.Push(luau.NewVariableDeclaration(luau.TempID("value"), luau.Num(1)))
	}
	return statements
}

func BenchmarkTempIdsRepeatedHints(b *testing.B) {
	for _, count := range []int{100, 1000, 5000} {
		b.Run(strconv.Itoa(count), func(b *testing.B) {
			statements := repeatedTempStatements(count)
			b.ReportAllocs()
			b.ResetTimer()
			for b.Loop() {
				RenderAST(statements)
			}
		})
	}
}
