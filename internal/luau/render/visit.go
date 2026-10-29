package render

import "rotor/internal/luau"

type visitor struct {
	before func(luau.Node)
	after  func(luau.Node)
	// Rendering historically evaluated map-field values first and allocated
	// fallback temporary names in that order, including in unsolved iterators.
	renderOrder bool
}

func visitNodeOrList(nodeOrList luau.NodeOrList, vis *visitor) {
	switch x := nodeOrList.(type) {
	case nil:
	case *luau.List[luau.Expression]:
		x.ForEach(func(n luau.Expression) { visitNode(n, vis) })
	case *luau.List[luau.Statement]:
		x.ForEach(func(n luau.Statement) { visitNode(n, vis) })
	case *luau.List[luau.AnyIdentifier]:
		x.ForEach(func(n luau.AnyIdentifier) { visitNode(n, vis) })
	case *luau.List[luau.WritableExpression]:
		x.ForEach(func(n luau.WritableExpression) { visitNode(n, vis) })
	case *luau.List[luau.Node]:
		x.ForEach(func(n luau.Node) { visitNode(n, vis) })
	case *luau.List[*luau.MapField]:
		x.ForEach(func(n *luau.MapField) { visitNode(n, vis) })
	case *luau.List[luau.TypeNode]:
		x.ForEach(func(n luau.TypeNode) { visitNode(n, vis) })
	case *luau.List[luau.TypeTableField]:
		x.ForEach(func(n luau.TypeTableField) { visitNode(n, vis) })
	case luau.Node:
		visitNode(x, vis)
	}
}

// visitType visits an optional type child.
func visitType(node luau.TypeNode, vis *visitor) {
	if node != nil {
		visitNode(node, vis)
	}
}

func visitGenerics(generics []luau.TypeGeneric, vis *visitor) {
	for _, generic := range generics {
		visitType(generic.Default, vis)
	}
}

// visitSignature visits the type children of a function-like node in render
// order: generic defaults, parameters (with their binding annotations), the
// varargs annotation, then the return annotation.
func visitSignature(generics []luau.TypeGeneric, node luau.HasParameters, vis *visitor) {
	visitGenerics(generics, vis)
	params, _, varArgsType := node.ParamData()
	visitNodeOrList(params, vis)
	visitType(varArgsType, vis)
	visitType(node.ReturnTypeData(), vis)
}

func visitNode(node luau.Node, vis *visitor) {
	if vis.before != nil {
		vis.before(node)
	}
	switch n := node.(type) {
	case *luau.Identifier:
		visitType(n.Annotation, vis)
	case *luau.TemporaryIdentifier:
		visitType(n.Annotation, vis)
	case *luau.ComputedIndexExpression:
		visitNode(n.Expression, vis)
		visitNode(n.Index, vis)
	case *luau.PropertyAccessExpression:
		visitNode(n.Expression, vis)
	case *luau.CallExpression:
		visitNode(n.Expression, vis)
		visitNodeOrList(n.Args, vis)
	case *luau.MethodCallExpression:
		visitNode(n.Expression, vis)
		visitNodeOrList(n.Args, vis)
	case *luau.ParenthesizedExpression:
		visitNode(n.Expression, vis)
	case *luau.FunctionExpression:
		visitSignature(n.Generics, n, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.BinaryExpression:
		visitNode(n.Left, vis)
		visitNode(n.Right, vis)
	case *luau.UnaryExpression:
		visitNode(n.Expression, vis)
	case *luau.IfExpression:
		visitNode(n.Condition, vis)
		visitNode(n.Expression, vis)
		visitNode(n.Alternative, vis)
	case *luau.InterpolatedString:
		visitNodeOrList(n.Parts, vis)
	case *luau.Array:
		visitNodeOrList(n.Members, vis)
	case *luau.Map:
		visitNodeOrList(n.Fields, vis)
	case *luau.Set:
		visitNodeOrList(n.Members, vis)
	case *luau.MixedTable:
		visitNodeOrList(n.Fields, vis)
	case *luau.CastExpression:
		visitNode(n.Expression, vis)
		visitNode(n.Type, vis)
	case *luau.Assignment:
		visitNodeOrList(n.Left, vis)
		visitNodeOrList(n.Right, vis)
	case *luau.CallStatement:
		visitNode(n.Expression, vis)
	case *luau.DoStatement:
		visitNodeOrList(n.Statements, vis)
	case *luau.WhileStatement:
		visitNode(n.Condition, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.RepeatStatement:
		visitNodeOrList(n.Statements, vis)
		visitNode(n.Condition, vis)
	case *luau.IfStatement:
		visitNode(n.Condition, vis)
		visitNodeOrList(n.Statements, vis)
		visitNodeOrList(n.ElseBody, vis)
	case *luau.NumericForStatement:
		visitNode(n.ID, vis)
		visitNode(n.Start, vis)
		visitNode(n.End, vis)
		if n.Step != nil {
			visitNode(n.Step, vis)
		}
		visitNodeOrList(n.Statements, vis)
	case *luau.ForStatement:
		visitNodeOrList(n.IDs, vis)
		if vis.renderOrder {
			visitNode(n.Expression, vis)
		}
		visitNodeOrList(n.Statements, vis)
	case *luau.FunctionDeclaration:
		visitNode(n.Name, vis)
		visitSignature(n.Generics, n, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.MethodDeclaration:
		visitNode(n.Expression, vis)
		visitSignature(n.Generics, n, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.VariableDeclaration:
		visitNodeOrList(n.Left, vis)
		visitNodeOrList(n.Right, vis)
	case *luau.ReturnStatement:
		visitNodeOrList(n.Expression, vis)
	case *luau.TypeAlias:
		visitGenerics(n.Generics, vis)
		visitType(n.Body, vis)
	case *luau.TypeFunctionDeclaration:
		visitNodeOrList(n.Parameters, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.MapField:
		if vis.renderOrder {
			visitNode(n.Value, vis)
			visitNode(n.Index, vis)
		} else {
			visitNode(n.Index, vis)
			visitNode(n.Value, vis)
		}
	// types
	case *luau.TypeReference:
		if n.Arguments != nil {
			visitNodeOrList(n.Arguments, vis)
		}
	case *luau.TypeOptional:
		visitNode(n.Inner, vis)
	case *luau.TypeUnion:
		visitNodeOrList(n.Members, vis)
	case *luau.TypeIntersection:
		visitNodeOrList(n.Members, vis)
	case *luau.TypeTable:
		visitNodeOrList(n.Fields, vis)
	case *luau.TypeTableProp:
		visitType(n.Value, vis)
	case *luau.TypeTableIndexer:
		visitType(n.Key, vis)
		visitType(n.Value, vis)
	case *luau.TypeArray:
		visitType(n.Element, vis)
	case *luau.TypeFunction:
		visitGenerics(n.Generics, vis)
		for _, parameter := range n.Parameters {
			visitType(parameter.Type, vis)
		}
		visitType(n.VarArgs, vis)
		visitType(n.Return, vis)
	case *luau.TypeTypeof:
		visitNode(n.Expression, vis)
	case *luau.TypePack:
		visitNodeOrList(n.Elements, vis)
	case *luau.TypeVariadic:
		visitNode(n.Inner, vis)
	}
	if vis.after != nil {
		vis.after(node)
	}
}
