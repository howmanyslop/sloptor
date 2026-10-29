package luau

// ---- types (rotor extension) ----
//
// Every production of the Luau type grammar
// (docs/superpowers/research/luau-type-system-digest.md section 1), plus the
// cast expression and the two type statements. Constructors adopt their
// children and never set an origin. The renderer owns parenthesization
// (render/types.go), so builders never inspect rendered text.

// TypeAccess is a table property or indexer modifier: `read x: T`,
// `write [K]: V`, `{ read T }`.
type TypeAccess uint8

const (
	TypeAccessNone TypeAccess = iota
	TypeAccessRead
	TypeAccessWrite
)

// TypeGeneric is one generic parameter of an alias, a function, or a function
// type: `T`, `U...` (IsPack), `T = number` (Default). Defaults are legal only
// on a TypeAlias; the renderer panics anywhere else.
type TypeGeneric struct {
	Name    string
	IsPack  bool
	Default TypeNode
}

// TypeFunctionParameter is one bound parameter of a function type. An empty
// Name renders the type alone.
type TypeFunctionParameter struct {
	Name string
	Type TypeNode
}

// TypeReference is `Name`, `Module.Name`, or `Name<A, B...>`; `nil` is a
// reference too. A nil or empty Arguments list renders the bare name.
type TypeReference struct {
	base
	Module    string
	Name      string
	Arguments *List[TypeNode]
}

type TypeStringSingleton struct {
	base
	Value string
}

type TypeBooleanSingleton struct {
	base
	Value bool
}

// TypeOptional is `T?`. Inner must not be a pack.
type TypeOptional struct {
	base
	Inner TypeNode
}

// TypeUnion is `A | B`. Multiline (set by render.LayoutTypes on alias bodies)
// renders one member per line with a leading `|`.
type TypeUnion struct {
	base
	Members   *List[TypeNode]
	Multiline bool
}

type TypeIntersection struct {
	base
	Members *List[TypeNode]
}

// TypeTable is `{ a: number, [string]: boolean }`. Multiline (set by
// render.LayoutTypes) renders one field per line, like a Map.
type TypeTable struct {
	base
	Fields    *List[TypeTableField]
	Multiline bool
}

// TypeTableProp is `[read|write] name: T`; a name that is not a valid
// identifier (or is a keyword) renders as `["name"]`.
type TypeTableProp struct {
	base
	Access TypeAccess
	Name   string
	Value  TypeNode
}

// TypeTableIndexer is `[read|write] [K]: V`; a table type has at most one.
type TypeTableIndexer struct {
	base
	Access TypeAccess
	Key    TypeNode
	Value  TypeNode
}

// TypeArray is the array shorthand `{ T }`, or `{ read T }` with Access.
type TypeArray struct {
	base
	Access  TypeAccess
	Element TypeNode
}

// TypeFunction is `<T, U...>(name: T, ...V) -> R`. VarArgs is nil, a
// *TypeVariadic (`...V`), or a *TypeGenericPack (`U...`). Return is never nil:
// an empty TypePack is `()`.
type TypeFunction struct {
	base
	Generics   []TypeGeneric
	Parameters []TypeFunctionParameter
	VarArgs    TypeNode
	Return     TypeNode
}

// TypeTypeof is `typeof(expression)`; the expression is never evaluated.
type TypeTypeof struct {
	base
	Expression Expression
}

// TypePack is the explicit pack `(A, B)`, or `()` when empty. A pack-shaped
// element is legal only in last position.
type TypePack struct {
	base
	Elements *List[TypeNode]
}

// TypeVariadic is `...T`.
type TypeVariadic struct {
	base
	Inner TypeNode
}

// TypeGenericPack is `T...`.
type TypeGenericPack struct {
	base
	Name string
}

// TypeRaw renders Text verbatim and is never parenthesized. It bridges the
// native annotation builder, which still produces strings, and goes away with
// that builder.
type TypeRaw struct {
	base
	Text string
}

// ---- cast expression ----

// CastExpression is `expression :: Type` (Luau `asexp`). It is not indexable:
// wrap it in a ParenthesizedExpression to call or index the result.
type CastExpression struct {
	base
	Expression Expression
	Type       TypeNode
}

// ---- type statements ----

// TypeAlias is `[export] type Name<K, V = number> = Body`.
type TypeAlias struct {
	base
	Export   bool
	Name     string
	Generics []TypeGeneric
	Body     TypeNode
}

// TypeFunctionDeclaration is `[export] type function Name(params) ... end`.
// Its body is ordinary Luau run by the type checker (new solver only).
type TypeFunctionDeclaration struct {
	base
	Export       bool
	Name         string
	Parameters   *List[AnyIdentifier]
	HasDotDotDot bool
	Statements   *List[Statement]
}

// ---- Kind ----

func (*TypeReference) Kind() SyntaxKind           { return KindTypeReference }
func (*TypeStringSingleton) Kind() SyntaxKind     { return KindTypeStringSingleton }
func (*TypeBooleanSingleton) Kind() SyntaxKind    { return KindTypeBooleanSingleton }
func (*TypeOptional) Kind() SyntaxKind            { return KindTypeOptional }
func (*TypeUnion) Kind() SyntaxKind               { return KindTypeUnion }
func (*TypeIntersection) Kind() SyntaxKind        { return KindTypeIntersection }
func (*TypeTable) Kind() SyntaxKind               { return KindTypeTable }
func (*TypeArray) Kind() SyntaxKind               { return KindTypeArray }
func (*TypeFunction) Kind() SyntaxKind            { return KindTypeFunction }
func (*TypeTypeof) Kind() SyntaxKind              { return KindTypeTypeof }
func (*TypePack) Kind() SyntaxKind                { return KindTypePack }
func (*TypeVariadic) Kind() SyntaxKind            { return KindTypeVariadic }
func (*TypeGenericPack) Kind() SyntaxKind         { return KindTypeGenericPack }
func (*TypeRaw) Kind() SyntaxKind                 { return KindTypeRaw }
func (*TypeTableProp) Kind() SyntaxKind           { return KindTypeTableProp }
func (*TypeTableIndexer) Kind() SyntaxKind        { return KindTypeTableIndexer }
func (*CastExpression) Kind() SyntaxKind          { return KindCastExpression }
func (*TypeAlias) Kind() SyntaxKind               { return KindTypeAlias }
func (*TypeFunctionDeclaration) Kind() SyntaxKind { return KindTypeFunctionDeclaration }

// ---- shallowClone ----

func (n *TypeReference) shallowClone() Node           { c := *n; return &c }
func (n *TypeStringSingleton) shallowClone() Node     { c := *n; return &c }
func (n *TypeBooleanSingleton) shallowClone() Node    { c := *n; return &c }
func (n *TypeOptional) shallowClone() Node            { c := *n; return &c }
func (n *TypeUnion) shallowClone() Node               { c := *n; return &c }
func (n *TypeIntersection) shallowClone() Node        { c := *n; return &c }
func (n *TypeTable) shallowClone() Node               { c := *n; return &c }
func (n *TypeArray) shallowClone() Node               { c := *n; return &c }
func (n *TypeFunction) shallowClone() Node            { c := *n; return &c }
func (n *TypeTypeof) shallowClone() Node              { c := *n; return &c }
func (n *TypePack) shallowClone() Node                { c := *n; return &c }
func (n *TypeVariadic) shallowClone() Node            { c := *n; return &c }
func (n *TypeGenericPack) shallowClone() Node         { c := *n; return &c }
func (n *TypeRaw) shallowClone() Node                 { c := *n; return &c }
func (n *TypeTableProp) shallowClone() Node           { c := *n; return &c }
func (n *TypeTableIndexer) shallowClone() Node        { c := *n; return &c }
func (n *CastExpression) shallowClone() Node          { c := *n; return &c }
func (n *TypeAlias) shallowClone() Node               { c := *n; return &c }
func (n *TypeFunctionDeclaration) shallowClone() Node { c := *n; return &c }

// ---- category markers ----

func (*TypeReference) typeNode()        {}
func (*TypeStringSingleton) typeNode()  {}
func (*TypeBooleanSingleton) typeNode() {}
func (*TypeOptional) typeNode()         {}
func (*TypeUnion) typeNode()            {}
func (*TypeIntersection) typeNode()     {}
func (*TypeTable) typeNode()            {}
func (*TypeArray) typeNode()            {}
func (*TypeFunction) typeNode()         {}
func (*TypeTypeof) typeNode()           {}
func (*TypePack) typeNode()             {}
func (*TypeVariadic) typeNode()         {}
func (*TypeGenericPack) typeNode()      {}
func (*TypeRaw) typeNode()              {}

func (*TypeTableProp) typeTableFieldNode()    {}
func (*TypeTableIndexer) typeTableFieldNode() {}

func (*CastExpression) expressionNode() {}

func (*TypeAlias) statementNode()               {}
func (*TypeFunctionDeclaration) statementNode() {}

// ---- HasParameters ----

func (n *TypeFunctionDeclaration) ParamData() (*List[AnyIdentifier], bool, TypeNode) {
	return n.Parameters, n.HasDotDotDot, nil
}

func (n *TypeFunctionDeclaration) ReturnTypeData() TypeNode { return nil }

// ---- constructors ----

func adoptGenerics(parent Node, generics []TypeGeneric) []TypeGeneric {
	for index := range generics {
		if generics[index].Default != nil {
			generics[index].Default = adopt[TypeNode](parent, generics[index].Default)
		}
	}
	return generics
}

// NewTypeReference builds `name` or `name<arguments>`; arguments may be nil.
func NewTypeReference(name string, arguments *List[TypeNode]) *TypeReference {
	n := &TypeReference{Name: name}
	n.Arguments = adoptList(n, arguments)
	return n
}

// NewTypeQualifiedReference builds `module.name` or `module.name<arguments>`.
func NewTypeQualifiedReference(module, name string, arguments *List[TypeNode]) *TypeReference {
	n := &TypeReference{Module: module, Name: name}
	n.Arguments = adoptList(n, arguments)
	return n
}

// TypeName builds a bare reference: `number`, `nil`, `Part`, `T`.
func TypeName(name string) *TypeReference { return &TypeReference{Name: name} }

func NewTypeStringSingleton(value string) *TypeStringSingleton {
	return &TypeStringSingleton{Value: value}
}

func NewTypeBooleanSingleton(value bool) *TypeBooleanSingleton {
	return &TypeBooleanSingleton{Value: value}
}

func NewTypeOptional(inner TypeNode) *TypeOptional {
	n := &TypeOptional{}
	n.Inner = adopt[TypeNode](n, inner)
	return n
}

// OptionalType is NewTypeOptional with normalization: an optional stays as it
// is (`T??` is `T?`), and so do `nil`, `any`, and `unknown`, which already
// admit nil.
func OptionalType(inner TypeNode) TypeNode {
	switch t := inner.(type) {
	case *TypeOptional:
		return t
	case *TypeReference:
		if t.Module == "" && (t.Arguments == nil || t.Arguments.IsEmpty()) {
			switch t.Name {
			case "nil", "any", "unknown":
				return t
			}
		}
	}
	return NewTypeOptional(inner)
}

func NewTypeUnion(members *List[TypeNode]) *TypeUnion {
	n := &TypeUnion{}
	n.Members = adoptList(n, members)
	return n
}

func NewTypeIntersection(members *List[TypeNode]) *TypeIntersection {
	n := &TypeIntersection{}
	n.Members = adoptList(n, members)
	return n
}

func NewTypeTable(fields *List[TypeTableField]) *TypeTable {
	n := &TypeTable{}
	n.Fields = adoptList(n, fields)
	return n
}

func NewTypeTableProp(access TypeAccess, name string, value TypeNode) *TypeTableProp {
	n := &TypeTableProp{Access: access, Name: name}
	n.Value = adopt[TypeNode](n, value)
	return n
}

func NewTypeTableIndexer(access TypeAccess, key, value TypeNode) *TypeTableIndexer {
	n := &TypeTableIndexer{Access: access}
	n.Key = adopt[TypeNode](n, key)
	n.Value = adopt[TypeNode](n, value)
	return n
}

func NewTypeArray(element TypeNode) *TypeArray {
	n := &TypeArray{}
	n.Element = adopt[TypeNode](n, element)
	return n
}

// NewTypeFunction builds `<generics>(parameters, varArgs) -> returnType`.
// varArgs may be nil; returnType must not be (use an empty NewTypePack for
// `()`).
func NewTypeFunction(generics []TypeGeneric, parameters []TypeFunctionParameter, varArgs, returnType TypeNode) *TypeFunction {
	n := &TypeFunction{}
	n.Generics = adoptGenerics(n, generics)
	for index := range parameters {
		if parameters[index].Type != nil {
			parameters[index].Type = adopt[TypeNode](n, parameters[index].Type)
		}
	}
	n.Parameters = parameters
	if varArgs != nil {
		n.VarArgs = adopt[TypeNode](n, varArgs)
	}
	if returnType != nil {
		n.Return = adopt[TypeNode](n, returnType)
	}
	return n
}

func NewTypeTypeof(expression Expression) *TypeTypeof {
	n := &TypeTypeof{}
	n.Expression = adopt[Expression](n, expression)
	return n
}

func NewTypePack(elements *List[TypeNode]) *TypePack {
	n := &TypePack{}
	n.Elements = adoptList(n, elements)
	return n
}

func NewTypeVariadic(inner TypeNode) *TypeVariadic {
	n := &TypeVariadic{}
	n.Inner = adopt[TypeNode](n, inner)
	return n
}

func NewTypeGenericPack(name string) *TypeGenericPack { return &TypeGenericPack{Name: name} }

func NewTypeRaw(text string) *TypeRaw { return &TypeRaw{Text: text} }

// NewCast builds `expression :: t`.
func NewCast(expression Expression, t TypeNode) *CastExpression {
	n := &CastExpression{}
	n.Expression = adopt[Expression](n, expression)
	n.Type = adopt[TypeNode](n, t)
	return n
}

// NewTypeAlias builds `type name<generics> = body`. body may be nil so a
// recursive alias can be registered before its body is built; SetBody fills
// it in, and the renderer panics on an alias that never got one.
func NewTypeAlias(name string, generics []TypeGeneric, body TypeNode) *TypeAlias {
	n := &TypeAlias{Name: name}
	n.Generics = adoptGenerics(n, generics)
	if body != nil {
		n.Body = adopt[TypeNode](n, body)
	}
	return n
}

// SetBody adopts body as the alias body, replacing any previous one.
func (n *TypeAlias) SetBody(body TypeNode) {
	n.Body = adopt[TypeNode](n, body)
}

// NewTypeFunctionDeclaration builds `type function name(params) ... end`.
func NewTypeFunctionDeclaration(name string, params *List[AnyIdentifier], hasDotDotDot bool, statements *List[Statement]) *TypeFunctionDeclaration {
	n := &TypeFunctionDeclaration{Name: name, HasDotDotDot: hasDotDotDot}
	n.Parameters = adoptList(n, params)
	n.Statements = adoptList(n, statements)
	return n
}

// SetBindingType annotates an identifier where it is bound: a parameter or
// the left side of a `local`. The annotation lives on the identifier itself
// because constructors clone nodes that already have a parent (a parameter
// reused in a default-value check, say), and a clone keeps the annotation
// pointer. nil removes the annotation.
func SetBindingType(identifier AnyIdentifier, t TypeNode) {
	switch id := identifier.(type) {
	case *Identifier:
		id.Annotation = adopt[TypeNode](id, t)
	case *TemporaryIdentifier:
		id.Annotation = adopt[TypeNode](id, t)
	}
}

// BindingType returns the annotation set by SetBindingType, if any.
func BindingType(identifier AnyIdentifier) TypeNode {
	switch id := identifier.(type) {
	case *Identifier:
		return id.Annotation
	case *TemporaryIdentifier:
		return id.Annotation
	}
	return nil
}
