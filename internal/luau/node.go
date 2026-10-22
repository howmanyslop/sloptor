package luau

// NodeOrList is the union of any AST node and any *List[T] — mirrors upstream
// fields typed `luau.X | luau.List<luau.Y>`.
type NodeOrList interface{ nodeOrList() }

// Position is a 0-based source coordinate with a UTF-16 column.
type Position struct{ Line, Column int }

// Origin anchors a generated node to its source start and optional closing token.
// Shallow clones share the same origin.
type Origin struct {
	Start   Position
	Closing *Position
}

type Node interface {
	NodeOrList
	Kind() SyntaxKind
	Parent() Node
	Origin() *Origin
	setParent(Node)
	setOrigin(*Origin)
	shallowClone() Node
}

// Marker interfaces mirror upstream category types.
type Expression interface {
	Node
	expressionNode()
}

type IndexableExpression interface {
	Expression
	indexableNode()
}

type Statement interface {
	Node
	statementNode()
}

type FieldNode interface {
	Node
	fieldNode()
}

// AnyIdentifier = Identifier | TemporaryIdentifier
type AnyIdentifier interface {
	IndexableExpression
	anyIdentifierNode()
}

// WritableExpression = AnyIdentifier | PropertyAccessExpression | ComputedIndexExpression
type WritableExpression interface {
	Expression
	writableNode()
}

// HasParameters mirrors the upstream HasParameters interface.
type HasParameters interface {
	Node
	ParamData() (params *List[AnyIdentifier], hasDotDotDot bool)
}

// base is embedded in every node struct.
type base struct {
	parent Node
	origin *Origin
}

func (b *base) Parent() Node        { return b.parent }
func (b *base) Origin() *Origin     { return b.origin }
func (b *base) setParent(p Node)    { b.parent = p }
func (b *base) setOrigin(o *Origin) { b.origin = o }
func (b *base) nodeOrList()         {}

// SetOrigin tags node only if it has no origin, preserving more precise anchors.
func SetOrigin(node Node, origin *Origin) {
	if node.Origin() == nil {
		node.setOrigin(origin)
	}
}

// SetParent reparents node without cloning. Upstream transform code mutates
// `.parent` directly in places (e.g. transformForStatement.ts addFinalizers
// L47 fixes the parents of finalizer clones spliced before `continue`
// statements); this is the Go surface for those parent fixups.
func SetParent(node Node, parent Node) { node.setParent(parent) }
