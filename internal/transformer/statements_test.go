package transformer

import (
	"context"
	"path/filepath"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

// buildInternalState builds a program WITHOUT the tsconfig sanitizer (which
// lives in internal/compile and would import-cycle with this package) — the
// fixture's tsconfig must therefore be tsgo-clean (types: [], full lib).
func buildInternalState(t *testing.T, projDir, relPath string) *State {
	t.Helper()

	dir, err := filepath.Abs(projDir)
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)

	fs := bundled.WrapFS(osvfs.FS())
	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)

	parsed, configDiags := tsoptions.GetParsedCommandLineOfConfigFile(dir+"/tsconfig.json", nil, nil, host, nil)
	if len(configDiags) > 0 {
		t.Fatalf("config diagnostics: %v", configDiags)
	}

	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	ctx := context.Background()

	sourceFile := program.GetSourceFile(dir + "/" + filepath.ToSlash(relPath))
	if sourceFile == nil {
		t.Fatalf("source file not in program: %s", relPath)
	}
	for _, d := range program.GetSemanticDiagnostics(ctx, sourceFile) {
		t.Errorf("unexpected semantic diagnostic: %s", d.String())
	}
	if t.Failed() {
		t.FailNow()
	}

	chk, release := program.GetTypeChecker(ctx)
	t.Cleanup(release)

	return NewState(program, chk, sourceFile, NewDiagService(), NewMultiState())
}

// caseHoistFixture returns the state plus the case-block clauses of the
// fixture's switch statement.
func caseHoistFixture(t *testing.T) (*State, []*ast.Node) {
	t.Helper()
	s := buildInternalState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "casehoist"), "src/case.ts")

	fn := s.SourceFile.Statements.Nodes[0]
	if !ast.IsFunctionDeclaration(fn) {
		t.Fatalf("expected FunctionDeclaration first, got %v", fn.Kind)
	}
	var switchStatement *ast.Node
	for _, statement := range fn.AsFunctionDeclaration().Body.AsBlock().Statements.Nodes {
		if ast.IsSwitchStatement(statement) {
			switchStatement = statement
		}
	}
	if switchStatement == nil {
		t.Fatal("switch statement not found")
	}
	return s, switchStatement.AsSwitchStatement().CaseBlock.AsCaseBlock().Clauses.Nodes
}

// clauseDeclName returns the declaration name of the clause's first statement
// (a VariableStatement in the fixture).
func clauseDeclName(t *testing.T, clause *ast.Node) *ast.Node {
	t.Helper()
	statement := clause.AsCaseOrDefaultClause().Statements.Nodes[0]
	if !ast.IsVariableStatement(statement) {
		t.Fatalf("expected VariableStatement, got %v", statement.Kind)
	}
	return statement.AsVariableStatement().DeclarationList.
		AsVariableDeclarationList().Declarations.Nodes[0].AsVariableDeclaration().Name()
}

// TestCheckVariableHoistCrossClause: a declaration directly inside a case
// clause that is referenced from a sibling clause is hoisted — recorded
// against the declaring CaseClause (where the switch transform emits the
// `local x` before the clause's `if`), and memoized so a second call does
// not double-register.
func TestCheckVariableHoistCrossClause(t *testing.T) {
	s, clauses := caseHoistFixture(t)

	sharedName := clauseDeclName(t, clauses[0])
	symbol := s.Checker.GetSymbolAtLocation(sharedName)
	if symbol == nil {
		t.Fatal("no symbol for shared")
	}

	checkVariableHoist(s, sharedName, symbol)

	if !s.IsHoisted[symbol] {
		t.Error("IsHoisted[shared] = false, want true")
	}
	hoists := s.HoistsByStatement[clauses[0]]
	if len(hoists) != 1 || hoists[0] != sharedName {
		t.Errorf("HoistsByStatement[case 0] = %v, want [shared]", hoists)
	}

	// Memoized: the decision is never reconsidered.
	checkVariableHoist(s, sharedName, symbol)
	if len(s.HoistsByStatement[clauses[0]]) != 1 {
		t.Error("second checkVariableHoist call must not double-register")
	}
}

// TestCheckVariableHoistOwnClauseOnly: a case-clause declaration referenced
// only within its own clause records NO hoist decision.
func TestCheckVariableHoistOwnClauseOnly(t *testing.T) {
	s, clauses := caseHoistFixture(t)

	onlyMineName := clauseDeclName(t, clauses[2])
	symbol := s.Checker.GetSymbolAtLocation(onlyMineName)
	if symbol == nil {
		t.Fatal("no symbol for onlyMine")
	}

	checkVariableHoist(s, onlyMineName, symbol)

	if _, decided := s.IsHoisted[symbol]; decided {
		t.Error("IsHoisted[onlyMine] recorded, want no decision")
	}
	if len(s.HoistsByStatement[clauses[2]]) != 0 {
		t.Error("HoistsByStatement[case 2] must stay empty")
	}
}

// TestCheckVariableHoistDefaultClause: declarations in a DEFAULT clause are
// outside checkVariableHoist's scope (upstream tests ts.isCaseClause only).
func TestCheckVariableHoistDefaultClause(t *testing.T) {
	s, clauses := caseHoistFixture(t)

	defaultClause := clauses[len(clauses)-1]
	fallbackName := clauseDeclName(t, defaultClause)
	symbol := s.Checker.GetSymbolAtLocation(fallbackName)
	if symbol == nil {
		t.Fatal("no symbol for fallback")
	}

	checkVariableHoist(s, fallbackName, symbol)

	if _, decided := s.IsHoisted[symbol]; decided {
		t.Error("IsHoisted[fallback] recorded, want no decision")
	}
}

// TestCheckVariableHoistNonCaseScope: an ordinary block-scoped declaration
// (statement parent is not a CaseClause) records no decision.
func TestCheckVariableHoistNonCaseScope(t *testing.T) {
	s, _ := caseHoistFixture(t)

	fn := s.SourceFile.Statements.Nodes[0]
	resultName := fn.AsFunctionDeclaration().Body.AsBlock().Statements.Nodes[0].
		AsVariableStatement().DeclarationList.
		AsVariableDeclarationList().Declarations.Nodes[0].AsVariableDeclaration().Name()
	symbol := s.Checker.GetSymbolAtLocation(resultName)
	if symbol == nil {
		t.Fatal("no symbol for result")
	}

	checkVariableHoist(s, resultName, symbol)

	if _, decided := s.IsHoisted[symbol]; decided {
		t.Error("IsHoisted[result] recorded, want no decision")
	}
}
