package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// ---------------------------------------------------------------------------
// switch — statements/transformSwitchStatement.ts (COMPLETE)
// ---------------------------------------------------------------------------

// transformCaseClauseExpression builds a clause condition and the statements
// needed to evaluate its value once. When the previous clause can fall through:
//
//   - the case expression produced prereqs: they must only run when not
//     already falling through, so they are wrapped in
//     `if not _fallthrough then <prereqs>; _fallthrough = _exp == (<case>) end`
//     and the clause condition becomes just `_fallthrough`;
//   - no prereqs: condition = `_fallthrough or _exp == (<case>)`.
func transformCaseClauseExpression(
	s *State,
	caseClauseExpression *ast.Node,
	switchExpression luau.Expression,
	fallThroughFlagID *luau.TemporaryIdentifier,
	canFallThroughTo bool,
) (luau.Expression, *luau.List[luau.Statement]) {
	var expression luau.Expression
	prereqStatements := s.CaptureStatements(func() {
		expression = TransformExpression(s, caseClauseExpression)
	})

	if expressionMightMutate(s, expression, caseClauseExpression) {
		caseValueID := luau.TempID("caseValue")
		prereqStatements.Push(luau.NewVariableDeclaration(caseValueID, expression))
		expression = caseValueID
	}

	var condition luau.Expression = luau.NewBinary(switchExpression, "==", expression)

	if canFallThroughTo {
		if prereqStatements.IsNonEmpty() {
			noFallThroughCondition := luau.NewUnary("not", fallThroughFlagID)

			prereqStatements.Push(luau.NewAssignment(fallThroughFlagID, "=", condition))

			prereqStatements = luau.NewList[luau.Statement](
				luau.NewIf(noFallThroughCondition, prereqStatements, nil),
			)

			condition = fallThroughFlagID
		} else {
			condition = luau.NewBinary(fallThroughFlagID, "or", condition)
		}
	}

	return condition, prereqStatements
}

// caseClauseResult mirrors transformCaseClause's return object (L105-109).
type caseClauseResult struct {
	canFallThroughFrom bool
	prereqs            *luau.List[luau.Statement]
	clauseStatements   *luau.List[luau.Statement]
}

// transformCaseClause ports transformCaseClause (L54-110). Body shape: empty
// statements are filtered; a body that is exactly one Block dissolves the
// braces (transform the BLOCK's statements with the block as parent — case
// braces don't double-nest into `do ... end`). canFallThroughFrom = the body
// is empty or doesn't end in a final statement (return/break/continue); when
// it can fall through AND the next clause is a case clause
// (shouldUpdateFallThroughFlag), `_fallthrough = true` is appended.
//
// clauseStatements = [hoist declaration?, IfStatement]. The hoist declaration
// (`local x` for case-locals referenced from sibling clauses, registered by
// checkVariableHoist during the body transform above and keyed on this
// CaseClause) lands BEFORE the `if`, making the variables visible to later
// clauses.
func transformCaseClause(
	s *State,
	node *ast.Node,
	switchExpression luau.Expression,
	fallThroughFlagID *luau.TemporaryIdentifier,
	canFallThroughTo bool,
	shouldUpdateFallThroughFlag bool,
) caseClauseResult {
	clause := node.AsCaseOrDefaultClause()

	condition, prereqStatements := transformCaseClauseExpression(
		s, clause.Expression, switchExpression, fallThroughFlagID, canFallThroughTo,
	)

	nonEmptyStatements := make([]*ast.Node, 0, len(clause.Statements.Nodes))
	for _, statement := range clause.Statements.Nodes {
		if !ast.IsEmptyStatement(statement) {
			nonEmptyStatements = append(nonEmptyStatements, statement)
		}
	}
	var statements *luau.List[luau.Statement]
	if len(nonEmptyStatements) == 1 && ast.IsBlock(nonEmptyStatements[0]) {
		firstStatement := nonEmptyStatements[0]
		statements = TransformStatementList(s, firstStatement, firstStatement.AsBlock().Statements.Nodes, nil)
	} else {
		statements = TransformStatementList(s, node, clause.Statements.Nodes, nil)
	}

	canFallThroughFrom := statements.Tail == nil || !luau.IsFinalStatement(statements.Tail.Value)
	if canFallThroughFrom && shouldUpdateFallThroughFlag {
		statements.Push(luau.NewAssignment(fallThroughFlagID, "=", luau.Bool(true)))
	}

	clauseStatements := luau.NewList[luau.Statement]()

	if hoistDeclaration := createHoistDeclaration(s, node); hoistDeclaration != nil {
		clauseStatements.Push(hoistDeclaration)
	}

	clauseStatements.Push(luau.NewIf(condition, statements, nil))

	return caseClauseResult{
		canFallThroughFrom: canFallThroughFrom,
		prereqs:            prereqStatements,
		clauseStatements:   clauseStatements,
	}
}

// canCompareSwitchIdentifierDirectly keeps literal-only switches compact when
// the case block still resolves the operand to the same binding.
func canCompareSwitchIdentifierDirectly(s *State, node *ast.Node, expression luau.Expression) bool {
	switchStatement := node.AsSwitchStatement()
	identifier := SkipDownwards(switchStatement.Expression)
	if !ast.IsIdentifier(identifier) || expression.Kind() != luau.KindIdentifier {
		return false
	}
	if s.Checker.ResolveName(identifier.Text(), switchStatement.CaseBlock, ast.SymbolFlagsValue, false) !=
		s.Checker.GetSymbolAtLocation(identifier) {
		return false
	}
	for _, clause := range switchStatement.CaseBlock.AsCaseBlock().Clauses.Nodes {
		if ast.IsDefaultClause(clause) {
			continue
		}
		label := SkipDownwards(clause.AsCaseOrDefaultClause().Expression)
		switch label.Kind {
		case ast.KindStringLiteral, ast.KindNoSubstitutionTemplateLiteral, ast.KindNumericLiteral,
			ast.KindTrueKeyword, ast.KindFalseKeyword:
		default:
			return false
		}
	}
	return true
}

// transformSwitchStatement lowers the whole switch to `repeat ... until true`, with each case clause an
// `if` block inside (TS `break` -> plain Luau `break` exits the repeat).
//
//   - Mutable subjects are captured before case evaluation unless every label
//     is a literal and the case block resolves the same identifier. Subject
//     prereqs flow to the outer statement list, before the repeat.
//   - `local _fallthrough = false` is unshifted ONLY when some case clause can
//     fall through (isFallThroughFlagNeeded — set even when the falling clause
//     is the LAST case clause, which declares a never-read flag; verbatim).
//   - A DefaultClause's statements are emitted INLINE (no condition) and the
//     clause loop BREAKS: any clauses after a default are silently dropped
//     (upstream quirk — TS would still match them; port verbatim).
//
// PORT NOTE: a TS `continue` inside a switch inside a loop emits a Luau
// `continue` inside the repeat, which jumps to the `until` of the REPEAT
// (acting like a switch-break, not a loop-continue) — upstream emits this
// as-is; replicated byte-for-byte.
func transformSwitchStatement(s *State, node *ast.Node) *luau.List[luau.Statement] {
	switchStatement := node.AsSwitchStatement()

	// The `repeat ... until true` below IS a break boundary, so labels must
	// count it — otherwise `outer: for { switch { case 1: break outer } }`
	// would emit a bare `break` that only leaves the switch (rotor extension).
	s.EnterBreakScope()
	defer s.ExitBreakScope()

	expression := TransformExpression(s, switchStatement.Expression)
	if expressionMightMutate(s, expression, switchStatement.Expression) &&
		!canCompareSwitchIdentifierDirectly(s, node, expression) {
		expression = s.PushToVar(expression, "exp")
	} else {
		expression = s.PushToVarIfComplex(expression, "exp")
	}
	fallThroughFlagID := luau.TempID("fallthrough")

	isFallThroughFlagNeeded := false

	statements := luau.NewList[luau.Statement]()
	canFallThroughTo := false
	clauses := switchStatement.CaseBlock.AsCaseBlock().Clauses.Nodes
	for i, caseClauseNode := range clauses {
		if ast.IsCaseClause(caseClauseNode) {
			shouldUpdateFallThroughFlag := i < len(clauses)-1 && ast.IsCaseClause(clauses[i+1])
			result := transformCaseClause(
				s, caseClauseNode, expression, fallThroughFlagID, canFallThroughTo, shouldUpdateFallThroughFlag,
			)

			statements.PushList(result.prereqs)
			statements.PushList(result.clauseStatements)

			canFallThroughTo = result.canFallThroughFrom

			if result.canFallThroughFrom {
				isFallThroughFlagNeeded = true
			}
		} else {
			clause := caseClauseNode.AsCaseOrDefaultClause()
			statements.PushList(TransformStatementList(s, caseClauseNode, clause.Statements.Nodes, nil))
			break
		}
	}

	if isFallThroughFlagNeeded {
		statements.Unshift(luau.NewVariableDeclaration(fallThroughFlagID, luau.Bool(false)))
	}

	result := luau.NewList[luau.Statement](
		luau.NewRepeat(luau.Bool(true), statements),
	)
	result.PushList(s.LabelChecks())
	return result
}
