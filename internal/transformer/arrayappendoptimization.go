package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

type arrayAppendTarget struct {
	lengthID *luau.TemporaryIdentifier
}

type arrayAppendPlan struct {
	calls   map[*ast.Node]*arrayAppendTarget
	targets []*arrayAppendTarget
}

var isNullType = TypeCheck{check: func(t *checker.Type) bool {
	return t.Flags()&checker.TypeFlagsNull != 0
}}

func analyzeArrayAppendStatements(s *State, statements []*ast.Node) map[*ast.Node]*arrayAppendPlan {
	plans := make(map[*ast.Node]*arrayAppendPlan)
	if !s.OptimizedArrayAppends {
		return plans
	}

	for index := 1; index < len(statements); index++ {
		loop := statements[index]
		body := getArrayAppendLoopBody(loop)
		if body == nil {
			continue
		}

		definition := getFreshArrayDefinition(s, statements[index-1])
		if definition == nil {
			continue
		}

		target := &arrayAppendTarget{lengthID: luau.TempID(definition.Text() + "Length")}
		plan := &arrayAppendPlan{
			calls:   make(map[*ast.Node]*arrayAppendTarget),
			targets: []*arrayAppendTarget{target},
		}
		valid := true
		ForEachSymbolReference(s.Checker, definition, loop, func(reference *ast.Node) bool {
			call := getOptimizableArrayPushCall(s, body, reference)
			if call == nil {
				valid = false
				return true
			}
			if len(call.AsCallExpression().Arguments.Nodes) > 0 {
				plan.calls[call] = target
			}
			return false
		})

		if valid && len(plan.calls) > 0 {
			plans[loop] = plan
		}
	}
	return plans
}

func getFreshArrayDefinition(s *State, statement *ast.Node) *ast.Node {
	if !ast.IsVariableStatement(statement) || ast.HasSyntacticModifier(statement, ast.ModifierFlagsExport) {
		return nil
	}

	declarationList := statement.AsVariableStatement().DeclarationList
	if declarationList.Flags&ast.NodeFlagsConst == 0 || declarationList.Flags&ast.NodeFlagsUsing != 0 {
		return nil
	}
	declarations := declarationList.AsVariableDeclarationList().Declarations.Nodes
	if len(declarations) != 1 {
		return nil
	}

	declaration := declarations[0].AsVariableDeclaration()
	if !ast.IsIdentifier(declaration.Name()) || !isFreshEmptyArray(s, declaration.Initializer) {
		return nil
	}
	return declaration.Name()
}

func isFreshEmptyArray(s *State, initializer *ast.Node) bool {
	if initializer == nil {
		return false
	}
	initializer = SkipDownwards(initializer)
	if ast.IsArrayLiteralExpression(initializer) {
		return len(initializer.AsArrayLiteralExpression().Elements.Nodes) == 0
	}
	if !ast.IsNewExpression(initializer) {
		return false
	}

	newExpression := initializer.AsNewExpression()
	if newExpression.Arguments != nil && len(newExpression.Arguments.Nodes) != 0 {
		return false
	}
	constructSymbol := getFirstConstructSymbol(s, newExpression.Expression)
	return constructSymbol != nil &&
		s.Macros().GetConstructorMacro(constructSymbol) != nil &&
		IsArrayType(s).Check(s.GetType(initializer))
}

func getArrayAppendLoopBody(node *ast.Node) *ast.Node {
	for ast.IsLabeledStatement(node) {
		node = node.AsLabeledStatement().Statement
	}
	switch node.Kind {
	case ast.KindForStatement:
		return node.AsForStatement().Statement
	case ast.KindForOfStatement:
		return node.AsForInOrOfStatement().Statement
	case ast.KindWhileStatement:
		return node.AsWhileStatement().Statement
	case ast.KindDoStatement:
		return node.AsDoStatement().Statement
	default:
		return nil
	}
}

func getOptimizableArrayPushCall(s *State, body, reference *ast.Node) *ast.Node {
	if !isAncestorOf(body, reference) {
		return nil
	}
	for ancestor := reference.Parent; ancestor != nil && ancestor != body; ancestor = ancestor.Parent {
		if ast.IsFunctionLike(ancestor) {
			return nil
		}
	}

	propertyAccessNode := reference.Parent
	if !ast.IsPropertyAccessExpression(propertyAccessNode) {
		return nil
	}
	propertyAccess := propertyAccessNode.AsPropertyAccessExpression()
	if propertyAccess.Expression != reference || propertyAccess.QuestionDotToken != nil || propertyAccess.Name().Text() != "push" {
		return nil
	}

	callNode := propertyAccessNode.Parent
	if !ast.IsCallExpression(callNode) {
		return nil
	}
	call := callNode.AsCallExpression()
	if SkipDownwards(call.Expression) != propertyAccessNode || call.QuestionDotToken != nil {
		return nil
	}

	callType := s.Checker.GetNonOptionalType(s.GetType(call.Expression))
	callSymbol := GetFirstDefinedSymbol(s, callType)
	macro := s.Macros().GetPropertyCallMacro(callSymbol)
	if macro == nil || macro.Name != "Array.push" {
		return nil
	}

	for _, argument := range call.Arguments.Nodes {
		if ast.IsSpreadElement(argument) || IsPossiblyType(s, s.GetType(argument), IsUndefinedType, isNullType) {
			return nil
		}
	}
	return callNode
}
