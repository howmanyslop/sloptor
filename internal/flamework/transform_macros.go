package flamework

import "rotor/tsgo/ast"

func transformFlameworkCall(state *TransformState, node *ast.Node, runtime MacroRuntime) (MacroTransformResult, error) {
	if !ast.IsCallExpression(node) && !ast.IsNewExpression(node) {
		return MacroTransformResult{}, invalidMacro(node, "Flamework macro target must be a call or new expression")
	}
	signature := state.checker.GetResolvedSignature(node)
	if signature == nil {
		return MacroTransformResult{}, invalidMacro(node, "Flamework could not resolve the macro signature")
	}
	arguments := append([]*ast.Node(nil), node.Arguments()...)
	highest := -1
	macros := make(map[int]userMacro)
	for index := range macroParameterCount(state, signature) {
		if index < len(arguments) && !isUndefinedExpression(arguments[index]) {
			continue
		}
		target := state.checker.GetNonNullableType(state.checker.GetTypeAtPosition(signature, index))
		macro, ok, err := userMacroOfUnion(state, node, target)
		if err != nil {
			return MacroTransformResult{}, err
		}
		if ok {
			macros[index] = macro
			highest = index
		}
	}

	prerequisites := make([]*ast.Node, 0)
	for index := 0; index <= highest; index++ {
		macro, ok := macros[index]
		if !ok {
			if index >= len(arguments) {
				arguments = append(arguments, state.factory.NewIdentifier("undefined"))
			}
			continue
		}
		expression, statements, err := buildUserMacro(state, node, macro, runtime)
		if err != nil {
			return MacroTransformResult{}, err
		}
		for len(arguments) <= index {
			arguments = append(arguments, state.factory.NewIdentifier("undefined"))
		}
		arguments[index] = expression
		prerequisites = append(prerequisites, statements...)
	}

	metadata := readMacroMetadata(state, signature.Declaration())
	if err := transformNetworkingMiddlewareIntrinsic(state, signature, arguments, metadata.symbols["intrinsic-middleware"], runtime); err != nil {
		return MacroTransformResult{}, err
	}
	if symbols := metadata.symbols["intrinsic-inline"]; len(symbols) == 1 {
		expression, err := inlineMacroIntrinsic(state, signature, arguments, symbols[0])
		return MacroTransformResult{Expression: expression, Prerequisites: prerequisites}, err
	}
	if err := validateParameterConstIntrinsic(node, signature, metadata.symbols["intrinsic-const"]); err != nil {
		return MacroTransformResult{}, err
	}
	if metadata.requested("intrinsic-arg-shift") && len(arguments) > 0 {
		arguments = arguments[1:]
	}

	callee := node.Expression()
	imports := make([]MacroImport, 0, 1)
	if symbols := metadata.symbols["intrinsic-flamework-rewrite"]; len(symbols) > 0 && symbols[0].Parent != nil {
		namespace := symbols[0].Parent.Name
		callee = state.factory.NewElementAccessExpression(state.factory.NewIdentifier(namespace), nil, state.factory.NewStringLiteral(symbols[0].Name, ast.TokenFlagsNone), ast.NodeFlagsNone)
		imports = append(imports, MacroImport{Module: flameworkCoreModule, Export: namespace, Local: namespace})
	}
	argumentList := state.factory.NewNodeList(arguments)
	if ast.IsCallExpression(node) {
		call := node.AsCallExpression()
		updated := state.factory.UpdateCallExpression(call, callee, call.QuestionDotToken, call.TypeArguments, argumentList, call.Flags)
		return MacroTransformResult{Expression: updated, Prerequisites: prerequisites, Imports: imports}, nil
	}
	constructor := node.AsNewExpression()
	updated := state.factory.UpdateNewExpression(constructor, callee, constructor.TypeArguments, argumentList)
	return MacroTransformResult{Expression: updated, Prerequisites: prerequisites, Imports: imports}, nil
}

func buildUserMacro(state *TransformState, trace *ast.Node, macro userMacro, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	var expression *ast.Node
	var prerequisites []*ast.Node
	var err error
	switch macro.kind {
	case macroGeneric:
		expression, prerequisites, err = buildGenericMacro(state, trace, macro, runtime)
	case macroCaller:
		expression, err = buildCallerMacro(state, trace, macro, runtime)
	case macroMany:
		expression, prerequisites, err = buildManyMacro(state, trace, macro, runtime)
	case macroLiteral:
		expression = literalExpression(state.factory, macro.literal)
	case macroIntrinsic:
		expression, prerequisites, err = buildIntrinsicMacro(state, trace, macro, runtime)
	default:
		err = invalidMacro(trace, "Flamework encountered an unknown macro kind")
	}
	if err != nil {
		return nil, nil, err
	}
	return asNever(state.factory, expression), prerequisites, nil
}

func buildGenericMacro(state *TransformState, trace *ast.Node, macro userMacro, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	switch macro.metadata {
	case "id":
		id, err := typeUID(state, macro.target, trace)
		return state.factory.NewStringLiteral(id, ast.TokenFlagsNone), nil, err
	case "text":
		return state.factory.NewStringLiteral(state.checker.TypeToString(macro.target), ast.TokenFlagsNone), nil, nil
	case "guard":
		if runtime.BuildGuard == nil {
			return nil, nil, ErrGuardBuilderAbsent
		}
		result, err := runtime.BuildGuard(state, trace, macro.target)
		return result.Expression, result.Statements, err
	default:
		return state.factory.NewIdentifier("undefined"), nil, nil
	}
}
