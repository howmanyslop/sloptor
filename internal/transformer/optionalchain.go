package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports the optional path of nodes/transformOptionalChain.ts
// (L192-348). flattenOptionalChain, chainItem, and transformChainItem live in
// call.go alongside the call transforms they dispatch to.

// createOrSetTempId ports createOrSetTempId (L192-217): create the chain temp
// on first use (named after the outermost node's VariableDeclaration parent,
// falling back to "result"), or reassign it — skipping the self-assignment
// when the chain result already lives in the temp.
func createOrSetTempId(s *State, tempId *luau.TemporaryIdentifier, expression luau.Expression, node *ast.Node) *luau.TemporaryIdentifier {
	if tempId == nil {
		name := "result"
		if node.Parent != nil && ast.IsVariableDeclaration(node.Parent) {
			if declName := node.Parent.AsVariableDeclaration().Name(); declName != nil && ast.IsIdentifier(declName) {
				name = declName.Text()
			}
		}
		tempId = s.PushToVar(expression, name)
	} else {
		if luau.Expression(tempId) != expression {
			s.Prereq(luau.NewAssignment(tempId, "=", expression))
		}
	}
	return tempId
}

// createNilCheck ports createNilCheck (L219-225): `if tempId ~= nil then
// <statements> end`.
func createNilCheck(s *State, tempId *luau.TemporaryIdentifier, statements *luau.List[luau.Statement], token *ast.Node) luau.Statement {
	condition := luau.NewBinary(tempId, "~=", luau.Nil())
	s.setOperatorSourcePosition(condition, token)
	return luau.NewIf(
		condition,
		statements,
		luau.NewList[luau.Statement](),
	)
}

// isCompoundCall ports isCompoundCall (L227-229).
func isCompoundCall(item chainItem) bool {
	return item.kind == chainPropertyCall || item.kind == chainElementCall
}

// transformOptionalChainInner ports transformOptionalChainInner (L231-348).
// ONE temp threads the whole chain: created at the first optional link and
// reassigned inside each nil-check block; deeper links nest INSIDE the
// current block via the inner capture. The statement order produced by the
// two-level capture is byte-load-bearing.
func transformOptionalChainInner(
	s *State,
	chain []chainItem,
	baseExpression luau.Expression,
	tempId *luau.TemporaryIdentifier,
	index int,
) luau.Expression {
	if index >= len(chain) {
		return baseExpression
	}
	item := chain[index]
	if item.optional || (isCompoundCall(item) && item.callOptional) {
		isMethodCall := false
		isSuperCall := false
		var selfParam *luau.TemporaryIdentifier

		if isCompoundCall(item) {
			isMethodCall = isMethod(s, item.expression)
			isSuperCall = ast.IsSuperProperty(item.expression)

			// a.b?.() on a real method binds the receiver to `_self` BEFORE
			// any nil check; super properties use the `self` global instead.
			if item.callOptional && isMethodCall && !isSuperCall {
				selfParam = s.PushToVar(baseExpression, "self")
				baseExpression = selfParam
			}

			if item.optional {
				tempId = createOrSetTempId(s, tempId, baseExpression, chain[len(chain)-1].node)
				baseExpression = tempId
			}

			if item.callOptional {
				if item.kind == chainPropertyCall {
					baseExpression = luau.NewPropertyAccess(convertToIndexableExpression(baseExpression), item.name)
				} else {
					expType := s.GetType(item.expression.AsElementAccessExpression().Expression)

					baseExpression = luau.NewComputedIndex(
						convertToIndexableExpression(baseExpression),
						addOneIfArrayType(s, expType, TransformExpression(s, item.argumentExpression)),
					)
				}
			}
		}

		// capture so we can wrap later if necessary
		result, prereqStatements := s.Capture(func() luau.Expression {
			tempId = createOrSetTempId(s, tempId, baseExpression, chain[len(chain)-1].node)

			newValue, ifStatements := s.Capture(func() luau.Expression {
				var newExpression luau.Expression
				if isCompoundCall(item) && item.callOptional {
					expType := s.Checker.GetNonOptionalType(s.GetType(item.node.AsCallExpression().Expression))
					if symbol := GetFirstDefinedSymbol(s, expType); symbol != nil {
						// registration is what matters: a registered entry
						// with a nil Macro is still a macro method and cannot
						// be optionally called.
						if entry := s.Macros().GetPropertyCallMacro(symbol); entry != nil {
							s.Diags.Add(DiagNoOptionalMacroCall(item.node))
							return luau.NewNone()
						}
					}

					args := ensureTransformOrder(s, item.args)
					if isMethodCall {
						if isSuperCall {
							args = append([]luau.Expression{luau.GlobalID("self")}, args...)
						} else {
							args = append([]luau.Expression{selfParam}, args...)
						}
					}
					newExpression = wrapReturnIfLuaTuple(s, item.node, luau.NewCall(tempId, luau.NewList(args...)))
				} else {
					newExpression = transformChainItem(s, tempId, item)
				}
				return transformOptionalChainInner(s, chain, newExpression, tempId, index+1)
			})

			isUsed := !luau.IsNone(newValue) && !isUsedAsStatement(item.node)

			if luau.Expression(tempId) != newValue && isUsed {
				ifStatements.Push(luau.NewAssignment(tempId, "=", newValue))
			} else {
				if luau.IsCall(newValue) {
					ifStatements.Push(luau.NewCallStatement(newValue))
				}
			}

			token := item.node.QuestionDotToken()
			if isCompoundCall(item) && !item.callOptional {
				token = item.expression.QuestionDotToken()
			}
			s.Prereq(createNilCheck(s, tempId, ifStatements, token))

			if isUsed {
				return tempId
			}
			return luau.NewNone()
		})
		if result == nil {
			// Unreachable: every branch above returns a node. Fail loudly
			// here rather than nil-dereffing downstream.
			panic("transformer: transformOptionalChainInner got nil expression") // upstream assert
		}

		if isCompoundCall(item) && item.optional && item.callOptional {
			// a?.b?.(): a SECOND nil check wraps everything captured above
			// (including the inner nil check for the call itself).
			s.Prereq(createNilCheck(s, tempId, prereqStatements, item.expression.QuestionDotToken()))
		} else {
			s.PrereqList(prereqStatements)
		}

		return result
	}
	return transformOptionalChainInner(s, chain, transformChainItem(s, baseExpression, item), tempId, index+1)
}

// transformOptionalChain ports transformOptionalChain (L350-356): every
// property/element access and call routes through here; fully non-optional
// chains degenerate to an inner-to-outer fold of transformChainItem.
func transformOptionalChain(s *State, node *ast.Node) luau.Expression {
	chain, expression := flattenOptionalChain(node)
	// rotor extension: the $env compile-time environment macro must consume
	// the base `$env` identifier BEFORE TransformExpression sees it (see
	// envmacro.go for why this is the interception point).
	if result, handled := interceptEnvChain(s, chain, expression); handled {
		return result
	}
	// rotor extension: the $nameof / $keys / $file
	// compile-time macros, same interception rationale (see nameofmacro.go,
	// keysmacro.go, filemacro.go).
	if result, handled := interceptNameofChain(s, chain, expression); handled {
		return result
	}
	if result, handled := interceptKeysChain(s, chain, expression); handled {
		return result
	}
	if result, handled := interceptFileChain(s, chain, expression); handled {
		return result
	}
	return transformOptionalChainInner(s, chain, TransformExpression(s, expression), nil, 0)
}
