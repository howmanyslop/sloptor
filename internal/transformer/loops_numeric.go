package transformer

import (
	"math"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

func isLoopVariable(s *State, expression *ast.Node, symbol *ast.Symbol) bool {
	expression = SkipDownwards(expression)
	return ast.IsIdentifier(expression) && s.Checker.GetSymbolAtLocation(expression) == symbol
}

func getOptimizedIncrementorStepValue(s *State, incrementor *ast.Node, idSymbol *ast.Symbol) (float64, bool) {
	incrementor = SkipDownwards(incrementor)
	if ast.IsBinaryExpression(incrementor) {
		binary := incrementor.AsBinaryExpression()
		if !isLoopVariable(s, binary.Left, idSymbol) {
			return 0, false
		}
		switch binary.OperatorToken.Kind {
		case ast.KindPlusEqualsToken:
			return getConstantLoopInteger(s, binary.Right, true)
		case ast.KindMinusEqualsToken:
			value, ok := getConstantLoopInteger(s, binary.Right, true)
			return -value, ok
		}
	}
	if ast.IsPostfixUnaryExpression(incrementor) || ast.IsPrefixUnaryExpression(incrementor) {
		operand, operator := unaryOperandAndOperator(incrementor)
		if isLoopVariable(s, operand, idSymbol) {
			switch operator {
			case ast.KindPlusPlusToken:
				return 1, true
			case ast.KindMinusMinusToken:
				return -1, true
			}
		}
	}
	return 0, false
}

func unaryOperandAndOperator(node *ast.Node) (*ast.Node, ast.Kind) {
	if ast.IsPrefixUnaryExpression(node) {
		unary := node.AsPrefixUnaryExpression()
		return unary.Operand, unary.Operator
	}
	unary := node.AsPostfixUnaryExpression()
	return unary.Operand, unary.Operator
}

func isMutatedInBody(s *State, identifier *ast.Node, body *ast.Node) bool {
	return ForEachSymbolReference(s.Checker, identifier, body, func(token *ast.Node) bool {
		return ast.IsWriteAccess(SkipUpwards(token))
	})
}

// Numeric-for hoists bounds and steps. Literal types alone cannot prove that
// their values are stable: properties and calls can change on every read.
func getConstantLoopInteger(s *State, expression *ast.Node, requireInitialized bool) (float64, bool) {
	type result struct {
		value float64
		ok    bool
	}
	cache := map[*ast.Node]result{}
	var getInteger func(*ast.Node) (float64, bool)
	getInteger = func(expression *ast.Node) (float64, bool) {
		expression = SkipDownwards(expression)
		if cached, exists := cache[expression]; exists {
			return cached.value, cached.ok
		}
		cache[expression] = result{}
		value, ok := getConstantLoopNumber(
			s,
			expression,
			requireInitialized,
			getInteger,
		)
		ok = ok && math.Abs(value) <= 9007199254740991 && value == math.Trunc(value)
		cache[expression] = result{value: value, ok: ok}
		return value, ok
	}
	return getInteger(expression)
}

func getConstantLoopNumber(
	s *State,
	expression *ast.Node,
	requireInitialized bool,
	getInteger func(*ast.Node) (float64, bool),
) (float64, bool) {
	switch expression.Kind {
	case ast.KindNumericLiteral:
		value, err := luau.JSNumberParse(getText(s, expression))
		return value, err == nil
	case ast.KindPrefixUnaryExpression:
		unary := expression.AsPrefixUnaryExpression()
		switch unary.Operator {
		case ast.KindPlusToken:
			return getInteger(unary.Operand)
		case ast.KindMinusToken:
			value, ok := getInteger(unary.Operand)
			return -value, ok
		}
	case ast.KindBinaryExpression:
		binary := expression.AsBinaryExpression()
		left, leftOK := getInteger(binary.Left)
		right, rightOK := getInteger(binary.Right)
		if !leftOK || !rightOK {
			return 0, false
		}
		switch binary.OperatorToken.Kind {
		case ast.KindPlusToken:
			return left + right, true
		case ast.KindMinusToken:
			return left - right, true
		case ast.KindAsteriskToken:
			return left * right, true
		case ast.KindSlashToken:
			return left / right, true
		case ast.KindAsteriskAsteriskToken:
			return math.Pow(left, right), true
		}
	case ast.KindIdentifier:
		declaration := getConstantLoopDeclaration(s, expression, requireInitialized)
		if declaration != nil {
			return getInteger(declaration.Initializer())
		}
	}
	return 0, false
}

func getConstantLoopDeclaration(s *State, expression *ast.Node, requireInitialized bool) *ast.Node {
	symbol := s.Checker.GetSymbolAtLocation(expression)
	if symbol == nil || symbol.ValueDeclaration == nil {
		return nil
	}
	declaration := symbol.ValueDeclaration
	if !ast.IsVariableDeclaration(declaration) || !ast.IsVarConst(declaration) || declaration.Initializer() == nil {
		return nil
	}
	if ast.GetSourceFileOfNode(declaration) != ast.GetSourceFileOfNode(expression) ||
		ast.GetCombinedModifierFlags(declaration)&ast.ModifierFlagsAmbient != 0 {
		return nil
	}
	// A source step may never run. In particular, its const binding can still
	// be uninitialized when a closure containing a loop is called.
	if requireInitialized {
		declarationStatement := declaration.Parent.Parent
		if declaration.End() > expression.Pos() ||
			!isAncestorOf(declarationStatement.Parent, expression) ||
			ast.FindAncestor(declaration, ast.IsFunctionLike) != ast.FindAncestor(expression, ast.IsFunctionLike) {
			return nil
		}
	}
	return declaration
}
