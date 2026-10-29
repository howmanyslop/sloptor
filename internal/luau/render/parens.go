package render

import "rotor/internal/luau"

// Luau operator precedence — https://www.lua.org/manual/5.1/manual.html#2.5.6
const ifExpressionPrecedence = 1

var unaryOperatorPrecedence = map[luau.UnaryOperator]int{
	"not": 7, "#": 7, "-": 7,
}

var binaryOperatorPrecedence = map[luau.BinaryOperator]int{
	"or": 1, "and": 2,
	"<": 3, ">": 3, "<=": 3, ">=": 3, "~=": 3, "==": 3,
	"..": 4, "+": 5, "-": 5,
	"*": 6, "/": 6, "//": 6, "%": 6,
	"^": 8,
}

func getPrecedence(node luau.Node) int {
	switch n := node.(type) {
	case *luau.IfExpression:
		return ifExpressionPrecedence
	case *luau.BinaryExpression:
		return binaryOperatorPrecedence[n.Operator]
	case *luau.UnaryExpression:
		return unaryOperatorPrecedence[n.Operator]
	}
	panic("getPrecedence: not an expression with precedence")
}

func needsParentheses(node luau.Node) bool {
	parent := node.Parent()
	if parent != nil && luau.IsExpressionWithPrecedence(parent) {
		nodePrec, parentPrec := getPrecedence(node), getPrecedence(parent)
		if nodePrec < parentPrec {
			return true
		}
		if nodePrec == parentPrec {
			if bin, ok := parent.(*luau.BinaryExpression); ok {
				return node == luau.Node(bin.Right)
			}
		}
	}
	return false
}

// castNeedsParentheses: `::` is the highest-precedence postfix (Luau `asexp
// ::= simpleexp ['::' Type]`), so a cast needs parentheses only as the left
// operand of `<`, where the type parser would read `T < y` as the start of a
// type argument list. A cast under another cast is parenthesized by the outer
// one (castOperandNeedsParentheses): `x :: any :: Foo` is a syntax error.
func castNeedsParentheses(node *luau.CastExpression) bool {
	parent, ok := node.Parent().(*luau.BinaryExpression)
	return ok && parent.Operator == "<" && luau.Node(parent.Left) == luau.Node(node)
}

// castOperandNeedsParentheses: the operand of `::` is a simpleexp, so a
// cast, binary, unary, or if-expression operand must be parenthesized
// (`(a + b) :: number`; bare `a + b :: number` casts only `b`).
func castOperandNeedsParentheses(operand luau.Expression) bool {
	if _, ok := operand.(*luau.CastExpression); ok {
		return true
	}
	return luau.IsExpressionWithPrecedence(operand)
}
