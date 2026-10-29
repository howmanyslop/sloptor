package render

import (
	"strconv"

	"rotor/internal/luau"
)

// tempScope mirrors upstream solveTempIds.ts Scope.
type tempScope struct {
	ids     map[string]struct{}
	lastTry map[string]int
	nextTry map[tempNameFamily]int
	parent  *tempScope
}

type tempNameFamily struct {
	base      string
	separator string
}

// newTempScope leaves maps nil; they are allocated on first write
// (reads of nil maps are safe), since most scopes never register an id.
func newTempScope(parent *tempScope) *tempScope {
	return &tempScope{parent: parent}
}

func (s *tempScope) addID(id string) {
	if s.ids == nil {
		s.ids = make(map[string]struct{})
	}
	s.ids[id] = struct{}{}
}

func (s *tempScope) setLastTry(input string, i int) {
	if s.lastTry == nil {
		s.lastTry = make(map[string]int)
	}
	s.lastTry[input] = i
}

func (s *tempScope) nextID(base, separator string) string {
	family := tempNameFamily{base, separator}
	i, ok := s.nextTry[family]
	if !ok {
		// A final-name counter is written only when that name is reserved,
		// so the legacy starting suffix for this base cannot later change.
		i, ok = s.lastTry[base]
		if !ok {
			i = 1
		}
	}
	input := base
	for s.has(input) {
		input = base + separator + strconv.Itoa(i)
		i++
	}
	// Preserve upstream's final-name counter: overlapping hints can read it.
	s.setLastTry(input, i)
	s.addID(input)
	if s.nextTry == nil {
		s.nextTry = make(map[tempNameFamily]int)
	}
	// All skipped candidates remain reserved in this scope or an ancestor.
	// Keep separate cursors for anonymous "_1" and exact "__1" suffixes.
	s.nextTry[family] = i
	return input
}

// has mirrors upstream scopeHasId: checks this scope and all ancestors.
func (s *tempScope) has(id string) bool {
	if _, ok := s.ids[id]; ok {
		return true
	}
	if s.parent != nil {
		return s.parent.has(id)
	}
	return false
}

func isFullyScopedNode(node luau.Node) bool {
	switch node.Kind() {
	case luau.KindForStatement, luau.KindNumericForStatement:
		return true
	}
	return luau.IsFunctionLike(node)
}

func identifierListContains(identifiers *luau.List[luau.AnyIdentifier], target *luau.Identifier) bool {
	if identifiers == nil {
		return false
	}
	for current := identifiers.Head; current != nil; current = current.Next {
		if current.Value == target {
			return true
		}
	}
	return false
}

func isIdentifierDeclaration(identifier *luau.Identifier) bool {
	switch parent := identifier.Parent().(type) {
	case *luau.VariableDeclaration:
		switch left := parent.Left.(type) {
		case *luau.Identifier:
			return left == identifier
		case *luau.List[luau.AnyIdentifier]:
			return identifierListContains(left, identifier)
		}
	case *luau.FunctionDeclaration:
		return parent.Localize && parent.Name == identifier ||
			identifierListContains(parent.Parameters, identifier)
	case *luau.FunctionExpression:
		return identifierListContains(parent.Parameters, identifier)
	case *luau.MethodDeclaration:
		return identifierListContains(parent.Parameters, identifier)
	case *luau.TypeFunctionDeclaration:
		return identifierListContains(parent.Parameters, identifier)
	case *luau.ForStatement:
		return identifierListContains(parent.IDs, identifier)
	case *luau.NumericForStatement:
		return parent.ID == identifier
	}
	return false
}

// isScopeEdge mirrors upstream: node is the head (head=true) or tail
// (head=false) statement of its parent's statements list, or of an
// IfStatement's else-list.
func isScopeEdge(node luau.Node, head bool) bool {
	parent := node.Parent()
	if parent == nil {
		return false
	}
	edgeOf := func(l *luau.List[luau.Statement]) luau.Statement {
		if head {
			if l.Head != nil {
				return l.Head.Value
			}
		} else {
			if l.Tail != nil {
				return l.Tail.Value
			}
		}
		return nil
	}
	if luau.HasStatements(parent) {
		if stmts := luau.StatementsOf(parent); stmts != nil {
			if edge := edgeOf(stmts); edge != nil && luau.Node(edge) == node {
				return true
			}
		}
	}
	// non-list elseBody would have the elseBody itself as a parent,
	// which would be a luau.IfStatement and handled above
	if ifStmt, ok := parent.(*luau.IfStatement); ok {
		if elseList, ok := ifStmt.ElseBody.(*luau.List[luau.Statement]); ok {
			if edge := edgeOf(elseList); edge != nil && luau.Node(edge) == node {
				return true
			}
		}
	}
	return false
}

// solveTempIDs ports reference/luau-ast/src/LuauRenderer/solveTempIds.ts:
// assigns rendered names to TemporaryIdentifier nodes, scope-aware so they
// never collide with source identifiers or other temps.
// Returns iterator subtrees skipped by the scoped walk for fallback handling.
func solveTempIDs(state *RenderState, ast luau.NodeOrList) (unsolvedIterators []luau.Expression) {
	var tempIDsToProcess []*luau.TemporaryIdentifier
	nodesToScopes := map[*luau.TemporaryIdentifier]*tempScope{}

	scopeStack := []*tempScope{newTempScope(nil)}
	peek := func() *tempScope { return scopeStack[len(scopeStack)-1] }
	push := func() { scopeStack = append(scopeStack, newTempScope(peek())) }
	pop := func() { scopeStack = scopeStack[:len(scopeStack)-1] }
	registerID := func(name string) { peek().addID(name) }
	reserveIdentifierUse := func(name string) {
		for scope := peek(); scope != nil; scope = scope.parent {
			scope.addID(name)
		}
	}

	vis := &visitor{
		before: func(node luau.Node) {
			if declaration, ok := node.(*luau.FunctionDeclaration); ok && declaration.Localize {
				if identifier, ok := declaration.Name.(*luau.Identifier); ok {
					registerID(identifier.Name)
				}
			}
			if isFullyScopedNode(node) {
				push()
			}
			if isScopeEdge(node, true) {
				push()
			}
			switch n := node.(type) {
			case *luau.ForStatement:
				unsolvedIterators = append(unsolvedIterators, n.Expression)
			case *luau.TemporaryIdentifier:
				nodesToScopes[n] = peek()
				tempIDsToProcess = append(tempIDsToProcess, n)
			case *luau.Identifier:
				if isIdentifierDeclaration(n) {
					registerID(n.Name)
				} else {
					reserveIdentifierUse(n.Name)
				}
			case *luau.VariableDeclaration:
				switch l := n.Left.(type) {
				case *luau.List[luau.AnyIdentifier]:
					l.ForEach(func(id luau.AnyIdentifier) {
						if ident, ok := id.(*luau.Identifier); ok {
							registerID(ident.Name)
						}
					})
				case *luau.Identifier:
					registerID(l.Name)
				}
			default:
				if luau.IsFunctionLike(node) {
					params, _, _ := node.(luau.HasParameters).ParamData()
					params.ForEach(func(id luau.AnyIdentifier) {
						if ident, ok := id.(*luau.Identifier); ok {
							registerID(ident.Name)
						}
					})
				}
			}
		},
		after: func(node luau.Node) {
			if isFullyScopedNode(node) {
				pop()
			}
			if isScopeEdge(node, false) {
				pop()
			}
		},
	}
	visitNodeOrList(ast, vis)

	for _, tempID := range tempIDsToProcess {
		if _, done := state.seenTempNodes[tempID.ID]; done {
			continue
		}
		scope, ok := nodesToScopes[tempID]
		if !ok {
			// Unreachable: every processed temp was registered with its scope
			// during the walk above.
			panic("render: solveTempIDs missing scope for temp id") // upstream assert
		}
		base := tempID.Name
		separator := "_"
		if !tempID.ExactName {
			base = "_" + base
			if tempID.Name == "" {
				separator = ""
			}
		}
		state.seenTempNodes[tempID.ID] = scope.nextID(base, separator)
	}
	return unsolvedIterators
}
