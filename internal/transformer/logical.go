package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports nodes/transformLogical.ts: `&&`, `||`, `??`.

// logicalChainItem ports the LogicalChainItem interface (L11-16).
type logicalChainItem struct {
	node       *ast.Node
	operator   *ast.Node // token after this item, or nil for the final item
	expression luau.Expression
	statements *luau.List[luau.Statement]
	inline     bool
}

// conditionBuilder ports the buildCondition callback shape: given the
// `_condition` temp and the TS node of the chain item just assigned to it,
// produce the if-statement condition that decides whether evaluation
// continues to the next item.
type conditionBuilder func(conditionID *luau.TemporaryIdentifier, node *ast.Node) luau.Expression

// flattenByOperator ports flattenByOperator (L23-31): left-recursive flatten
// of same-operator chains, `a && b && c` -> `[a, b, c]`. Parenthesized
// sub-chains do NOT flatten (no skipDownwards — verbatim upstream).
func flattenByOperator(node *ast.Node, operatorKind ast.Kind, collectOperators bool) ([]*ast.Node, []*ast.Node) {
	var result, operators []*ast.Node
	for ast.IsBinaryExpression(node) && node.AsBinaryExpression().OperatorToken.Kind == operatorKind {
		result = append([]*ast.Node{node.AsBinaryExpression().Right}, result...)
		if collectOperators {
			operators = append([]*ast.Node{node.AsBinaryExpression().OperatorToken}, operators...)
		}
		node = node.AsBinaryExpression().Left
	}
	return append([]*ast.Node{node}, result...), operators
}

// getLogicalChain ports getLogicalChain (L37-53): flatten, capture each
// item's transform, and mark inlineability. An item is inlineable only if it
// produced no prereqs AND — unless it is the last item — its Luau truthiness
// matches TS truthiness (no 0/NaN/"" possibility); the LAST item never needs
// a wrap because its value is returned, not tested.
func getLogicalChain(s *State, binaryExp *ast.Node, binaryOperatorKind ast.Kind, enableInlining bool) []logicalChainItem {
	nodes, operators := flattenByOperator(binaryExp, binaryOperatorKind, s.SourceMapEnabled)
	chain := make([]logicalChainItem, len(nodes))
	for index, node := range nodes {
		t := s.GetType(node)
		expression, statements := s.Capture(func() luau.Expression { return TransformExpression(s, node) })
		inline := false
		if enableInlining {
			willWrap := index < len(nodes)-1 && WillCreateTruthinessChecks(s, t)
			inline = statements.IsEmpty() && !willWrap
		}
		chain[index] = logicalChainItem{node: node, expression: expression, statements: statements, inline: inline}
		if index < len(operators) {
			chain[index].operator = operators[index]
		}
	}
	return chain
}

// buildLogicalChainPrereqs ports buildLogicalChainPrereqs (L58-94):
// recursively nested prereq statements for non-inlined chains:
//
//	[item0.statements]
//	local _condition = item0.expression
//	if buildCondition(_condition, item0.node) then
//	    [item1.statements]
//	    _condition = item1.expression
//	    if ... then ... end
//	end
func buildLogicalChainPrereqs(s *State, chain []logicalChainItem, conditionID *luau.TemporaryIdentifier, buildCondition conditionBuilder, index int) {
	expInfo := chain[index]
	s.PrereqList(expInfo.statements)
	if index == 0 {
		s.Prereq(luau.NewVariableDeclaration(conditionID, expInfo.expression))
	} else {
		s.Prereq(luau.NewAssignment(conditionID, "=", expInfo.expression))
	}
	if index+1 < len(chain) {
		statements := s.CaptureStatements(func() {
			buildLogicalChainPrereqs(s, chain, conditionID, buildCondition, index+1)
		})
		condition := s.tagConditionOperator(buildCondition(conditionID, expInfo.node), expInfo.operator)
		s.Prereq(luau.NewIf(condition, statements, nil))
	}
}

func (s *State) tagConditionOperator(condition luau.Expression, token *ast.Node) luau.Expression {
	if s.SourceMapEnabled {
		if position := s.getOriginalSourcePosition(token, false); position != nil {
			return luau.WithOrigin(condition, &luau.Origin{Start: *position, Operator: position}).(luau.Expression)
		}
	}
	return condition
}

// mergeInlineExpressions ports mergeInlineExpressions (L108-121): adjacent
// inline items merge into one binaryExpressionChain item.
func mergeInlineExpressions(s *State, chain []logicalChainItem, binaryOperator luau.BinaryOperator) []logicalChainItem {
	for i := 0; i < len(chain); i++ {
		if chain[i].inline {
			exps := []luau.Expression{chain[i].expression}
			var origins []*luau.Origin
			j := i + 1
			for j < len(chain) && chain[j].inline {
				if s.SourceMapEnabled {
					origins = append(origins, &luau.Origin{StartUnset: true, Operator: s.getOriginalSourcePosition(chain[i].operator, false)})
				}
				exps = append(exps, chain[j].expression)
				chain[i].operator = chain[j].operator
				chain = append(chain[:j], chain[j+1:]...)
			}
			chain[i].expression = binaryExpressionChain(exps, binaryOperator, origins...)
		}
	}
	return chain
}

// buildInlineConditionExpression ports buildInlineConditionExpression
// (L126-145): after merging, a single inline item returns directly (pure
// `a and b and c`); otherwise allocate the `_condition` temp and build the
// if-chain prereqs.
func buildInlineConditionExpression(s *State, node *ast.Node, tsBinaryOperator ast.Kind, luaBinaryOperator luau.BinaryOperator, buildCondition conditionBuilder) luau.Expression {
	chain := getLogicalChain(s, node, tsBinaryOperator, true)

	chain = mergeInlineExpressions(s, chain, luaBinaryOperator)

	// single inline at the end, no temp variable needed
	if len(chain) == 1 && chain[0].inline {
		return chain[0].expression
	}

	conditionID := luau.TempID("condition")
	buildLogicalChainPrereqs(s, chain, conditionID, buildCondition, 0)
	return conditionID
}

// transformLogical ports transformLogical (L147-167).
func transformLogical(s *State, node *ast.Node) luau.Expression {
	operatorKind := node.AsBinaryExpression().OperatorToken.Kind
	switch operatorKind {
	case ast.KindAmpersandAmpersandToken:
		return buildInlineConditionExpression(s, node, operatorKind, "and",
			func(conditionID *luau.TemporaryIdentifier, node *ast.Node) luau.Expression {
				return CreateTruthinessChecks(s, conditionID, node, s.GetType(node))
			})
	case ast.KindBarBarToken:
		return buildInlineConditionExpression(s, node, operatorKind, "or",
			func(conditionID *luau.TemporaryIdentifier, node *ast.Node) luau.Expression {
				return luau.NewUnary("not", CreateTruthinessChecks(s, conditionID, node, s.GetType(node)))
			})
	case ast.KindQuestionQuestionToken:
		buildCondition := func(conditionID *luau.TemporaryIdentifier, _ *ast.Node) luau.Expression {
			return luau.NewBinary(conditionID, "==", luau.Nil())
		}
		// `a or b` is truthiness-correct only when the result can't be a
		// legitimate `false` (Luau `or` would wrongly skip it).
		if !IsPossiblyType(s, s.GetType(node), IsBooleanLiteralType(s, false)) {
			return buildInlineConditionExpression(s, node, operatorKind, "or", buildCondition)
		}
		chain := getLogicalChain(s, node, ast.KindQuestionQuestionToken, false)
		conditionID := luau.TempID("condition")
		buildLogicalChainPrereqs(s, chain, conditionID, buildCondition, 0)
		return conditionID
	}
	panic("transformer: transformLogical operator not implemented: " + kindName(operatorKind)) // upstream assert(false)
}
