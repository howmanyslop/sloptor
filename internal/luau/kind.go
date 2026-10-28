package luau

type SyntaxKind uint8

const (
	// indexable expressions
	KindIdentifier SyntaxKind = iota
	KindTemporaryIdentifier
	KindComputedIndexExpression
	KindPropertyAccessExpression
	KindCallExpression
	KindMethodCallExpression
	KindParenthesizedExpression

	// expressions
	KindNone
	KindNilLiteral
	KindFalseLiteral
	KindTrueLiteral
	KindNumberLiteral
	KindStringLiteral
	KindVarArgsLiteral
	KindFunctionExpression
	KindBinaryExpression
	KindUnaryExpression
	KindIfExpression
	KindInterpolatedString
	KindArray
	KindMap
	KindSet
	KindMixedTable

	// statements
	KindAssignment
	KindBreakStatement
	KindCallStatement
	KindContinueStatement
	KindDoStatement
	KindWhileStatement
	KindRepeatStatement
	KindIfStatement
	KindNumericForStatement
	KindForStatement
	KindFunctionDeclaration
	KindMethodDeclaration
	KindVariableDeclaration
	KindReturnStatement
	KindComment

	// fields
	KindMapField
	KindInterpolatedStringPart

	FirstIndexableExpression = KindIdentifier
	LastIndexableExpression  = KindParenthesizedExpression
	FirstExpression          = KindIdentifier
	LastExpression           = KindMixedTable
	FirstStatement           = KindAssignment
	LastStatement            = KindComment
	FirstField               = KindMapField
	LastField                = KindInterpolatedStringPart
)

var kindNames = [...]string{
	KindIdentifier:               "Identifier",
	KindTemporaryIdentifier:      "TemporaryIdentifier",
	KindComputedIndexExpression:  "ComputedIndexExpression",
	KindPropertyAccessExpression: "PropertyAccessExpression",
	KindCallExpression:           "CallExpression",
	KindMethodCallExpression:     "MethodCallExpression",
	KindParenthesizedExpression:  "ParenthesizedExpression",
	KindNone:                     "None",
	KindNilLiteral:               "NilLiteral",
	KindFalseLiteral:             "FalseLiteral",
	KindTrueLiteral:              "TrueLiteral",
	KindNumberLiteral:            "NumberLiteral",
	KindStringLiteral:            "StringLiteral",
	KindVarArgsLiteral:           "VarArgsLiteral",
	KindFunctionExpression:       "FunctionExpression",
	KindBinaryExpression:         "BinaryExpression",
	KindUnaryExpression:          "UnaryExpression",
	KindIfExpression:             "IfExpression",
	KindInterpolatedString:       "InterpolatedString",
	KindArray:                    "Array",
	KindMap:                      "Map",
	KindSet:                      "Set",
	KindMixedTable:               "MixedTable",
	KindAssignment:               "Assignment",
	KindBreakStatement:           "BreakStatement",
	KindCallStatement:            "CallStatement",
	KindContinueStatement:        "ContinueStatement",
	KindDoStatement:              "DoStatement",
	KindWhileStatement:           "WhileStatement",
	KindRepeatStatement:          "RepeatStatement",
	KindIfStatement:              "IfStatement",
	KindNumericForStatement:      "NumericForStatement",
	KindForStatement:             "ForStatement",
	KindFunctionDeclaration:      "FunctionDeclaration",
	KindMethodDeclaration:        "MethodDeclaration",
	KindVariableDeclaration:      "VariableDeclaration",
	KindReturnStatement:          "ReturnStatement",
	KindComment:                  "Comment",
	KindMapField:                 "MapField",
	KindInterpolatedStringPart:   "InterpolatedStringPart",
}

func (k SyntaxKind) String() string { return kindNames[k] }

type (
	BinaryOperator     string // "+" "-" "*" "/" "//" "^" "%" ".." "<" "<=" ">" ">=" "==" "~=" "and" "or"
	UnaryOperator      string // "-" "not" "#"
	AssignmentOperator string // "=" "+=" "-=" "*=" "/=" "//=" "%=" "^=" "..="
)
