package luau

func IsIndexableExpression(n Node) bool {
	return n.Kind() >= FirstIndexableExpression && n.Kind() <= LastIndexableExpression
}
func IsExpression(n Node) bool { return n.Kind() >= FirstExpression && n.Kind() <= LastExpression }
func IsStatement(n Node) bool  { return n.Kind() >= FirstStatement && n.Kind() <= LastStatement }
func IsField(n Node) bool      { return n.Kind() >= FirstField && n.Kind() <= LastField }
func IsTypeNode(n Node) bool   { return n.Kind() >= FirstType && n.Kind() <= LastType }

// IsTypePack reports a pack-shaped type: `(A, B)`, `...T`, or `T...`. Packs
// are legal only as the last type argument, the last pack element, a function
// type's varargs, and a return.
func IsTypePack(n Node) bool {
	switch n.Kind() {
	case KindTypePack, KindTypeVariadic, KindTypeGenericPack:
		return true
	}
	return false
}

func IsSimple(n Node) bool {
	switch n.Kind() {
	case KindIdentifier, KindTemporaryIdentifier, KindNilLiteral, KindTrueLiteral,
		KindFalseLiteral, KindNumberLiteral, KindStringLiteral:
		return true
	}
	return false
}

func IsSimplePrimitive(n Node) bool {
	switch n.Kind() {
	case KindNilLiteral, KindTrueLiteral, KindFalseLiteral, KindNumberLiteral, KindStringLiteral:
		return true
	}
	return false
}

func IsTable(n Node) bool {
	switch n.Kind() {
	case KindArray, KindSet, KindMap, KindMixedTable:
		return true
	}
	return false
}

func IsFinalStatement(n Node) bool {
	switch n.Kind() {
	case KindBreakStatement, KindReturnStatement, KindContinueStatement:
		return true
	}
	return false
}

func IsCall(n Node) bool {
	return n.Kind() == KindCallExpression || n.Kind() == KindMethodCallExpression
}

func IsNone(n Node) bool {
	return n.Kind() == KindNone
}

func IsFunctionLike(n Node) bool {
	switch n.Kind() {
	case KindFunctionDeclaration, KindFunctionExpression, KindMethodDeclaration, KindTypeFunctionDeclaration:
		return true
	}
	return false
}

func HasStatements(n Node) bool {
	switch n.Kind() {
	case KindForStatement, KindNumericForStatement, KindFunctionExpression, KindDoStatement,
		KindFunctionDeclaration, KindIfStatement, KindMethodDeclaration, KindRepeatStatement,
		KindWhileStatement, KindTypeFunctionDeclaration:
		return true
	}
	return false
}

// StatementsOf returns the statement list of any HasStatements node.
func StatementsOf(n Node) *List[Statement] {
	switch x := n.(type) {
	case *ForStatement:
		return x.Statements
	case *NumericForStatement:
		return x.Statements
	case *FunctionExpression:
		return x.Statements
	case *DoStatement:
		return x.Statements
	case *FunctionDeclaration:
		return x.Statements
	case *IfStatement:
		return x.Statements
	case *MethodDeclaration:
		return x.Statements
	case *RepeatStatement:
		return x.Statements
	case *WhileStatement:
		return x.Statements
	case *TypeFunctionDeclaration:
		return x.Statements
	}
	return nil
}

func IsExpressionWithPrecedence(n Node) bool {
	switch n.Kind() {
	case KindIfExpression, KindUnaryExpression, KindBinaryExpression:
		return true
	}
	return false
}
