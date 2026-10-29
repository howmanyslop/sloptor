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
	// KindCastExpression is a rotor extension (roblox-ts never emits casts):
	// `expression :: Type`.
	KindCastExpression

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
	// KindTypeAlias and KindTypeFunctionDeclaration are rotor extensions
	// (roblox-ts has no type statements): `type Name<K, V> = <body>` and
	// `type function Name(params) ... end`.
	KindTypeAlias
	KindTypeFunctionDeclaration

	// fields
	KindMapField
	KindInterpolatedStringPart

	// types (rotor extension): every production of the Luau type grammar
	KindTypeReference
	KindTypeStringSingleton
	KindTypeBooleanSingleton
	KindTypeOptional
	KindTypeUnion
	KindTypeIntersection
	KindTypeTable
	KindTypeArray
	KindTypeFunction
	KindTypeTypeof
	KindTypePack
	KindTypeVariadic
	KindTypeGenericPack
	// KindTypeRaw carries pre-rendered type text while the native annotation
	// builder still produces strings; it goes away with that builder.
	KindTypeRaw

	// type table fields
	KindTypeTableProp
	KindTypeTableIndexer

	FirstIndexableExpression = KindIdentifier
	LastIndexableExpression  = KindParenthesizedExpression
	FirstExpression          = KindIdentifier
	LastExpression           = KindCastExpression
	FirstStatement           = KindAssignment
	LastStatement            = KindTypeFunctionDeclaration
	FirstField               = KindMapField
	LastField                = KindInterpolatedStringPart
	FirstType                = KindTypeReference
	LastType                 = KindTypeRaw
	FirstTypeField           = KindTypeTableProp
	LastTypeField            = KindTypeTableIndexer
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
	KindCastExpression:           "CastExpression",
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
	KindTypeAlias:                "TypeAlias",
	KindTypeFunctionDeclaration:  "TypeFunctionDeclaration",
	KindMapField:                 "MapField",
	KindInterpolatedStringPart:   "InterpolatedStringPart",
	KindTypeReference:            "TypeReference",
	KindTypeStringSingleton:      "TypeStringSingleton",
	KindTypeBooleanSingleton:     "TypeBooleanSingleton",
	KindTypeOptional:             "TypeOptional",
	KindTypeUnion:                "TypeUnion",
	KindTypeIntersection:         "TypeIntersection",
	KindTypeTable:                "TypeTable",
	KindTypeArray:                "TypeArray",
	KindTypeFunction:             "TypeFunction",
	KindTypeTypeof:               "TypeTypeof",
	KindTypePack:                 "TypePack",
	KindTypeVariadic:             "TypeVariadic",
	KindTypeGenericPack:          "TypeGenericPack",
	KindTypeRaw:                  "TypeRaw",
	KindTypeTableProp:            "TypeTableProp",
	KindTypeTableIndexer:         "TypeTableIndexer",
}

func (k SyntaxKind) String() string { return kindNames[k] }

type (
	BinaryOperator     string // "+" "-" "*" "/" "//" "^" "%" ".." "<" "<=" ">" ">=" "==" "~=" "and" "or"
	UnaryOperator      string // "-" "not" "#"
	AssignmentOperator string // "=" "+=" "-=" "*=" "/=" "//=" "%=" "^=" "..="
)
