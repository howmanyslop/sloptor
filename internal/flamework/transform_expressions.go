package flamework

import (
	"crypto/rand"
	"errors"
	"fmt"
	"math/big"

	"rotor/tsgo/ast"
)

var (
	ErrDynamicObfuscatedAccess = errors.New("flamework key-obfuscated object must be accessed directly")
	ErrDirectAttributeUpdate   = errors.New("flamework attribute update requires an instance access")
	ErrInvalidMacroRandomIndex = errors.New("invalid Flamework macro random index")
)

type expressionTransformResult struct {
	expression    *ast.Node
	prerequisites []*ast.Node
	imports       []MacroImport
}

func transformFlameworkExpression(state *TransformState, node *ast.Node) (*ast.Node, error) {
	result, err := transformFlameworkExpressionWithRuntime(state, node, defaultFlameworkMacroRuntime())
	return result.expression, err
}

func transformFlameworkExpressionWithRuntime(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	if state == nil || state.factory == nil || state.checker == nil || state.project == nil {
		return expressionTransformResult{}, fmt.Errorf("%w: expression transform state is incomplete", ErrInvalidTransformInput)
	}
	if node == nil {
		return expressionTransformResult{}, fmt.Errorf("%w: expression is nil", ErrInvalidTransformInput)
	}

	switch node.Kind {
	case ast.KindCallExpression:
		return transformFlameworkCallExpression(state, node, runtime)
	case ast.KindNewExpression:
		return transformFlameworkNewExpression(state, node, runtime)
	case ast.KindPrefixUnaryExpression, ast.KindPostfixUnaryExpression:
		return transformFlameworkUnaryExpression(state, node, runtime)
	case ast.KindBinaryExpression:
		return transformFlameworkBinaryExpression(state, node, runtime)
	case ast.KindElementAccessExpression, ast.KindPropertyAccessExpression:
		return transformFlameworkAccessExpression(state, node, runtime)
	case ast.KindDeleteExpression:
		return transformFlameworkDeleteExpression(state, node, runtime)
	default:
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}
}

func transformFlameworkExpressionChildrenWithRuntime(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	// Identifiers, literals, and keywords have nothing to visit, and they are
	// a large share of all expressions. Returning them directly saves a
	// visitor and its closure per node.
	if !node.ForEachChild(func(*ast.Node) bool { return true }) {
		return expressionTransformResult{expression: node, prerequisites: []*ast.Node{}, imports: []MacroImport{}}, nil
	}
	if state.expressionChildren == nil {
		children := &flameworkExpressionChildVisitor{state: state}
		children.visitor = ast.NewNodeVisitor(children.visit, state.factory, ast.NodeVisitorHooks{})
		state.expressionChildren = children
	}
	return state.expressionChildren.transform(node, runtime)
}

type flameworkExpressionChildFrame struct {
	runtime       MacroRuntime
	err           error
	prerequisites []*ast.Node
	imports       []MacroImport
}

type flameworkExpressionChildVisitor struct {
	state   *TransformState
	visitor *ast.NodeVisitor
	frames  []flameworkExpressionChildFrame
}

func (v *flameworkExpressionChildVisitor) transform(node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	index := len(v.frames)
	v.frames = append(v.frames, flameworkExpressionChildFrame{
		runtime: runtime, prerequisites: []*ast.Node{}, imports: []MacroImport{},
	})
	defer func() {
		// Drop invocation references on success, error, and panic. Result backing
		// arrays belong to the caller and must never be reused with the frame.
		v.frames[index] = flameworkExpressionChildFrame{}
		v.frames = v.frames[:index]
	}()
	transformed := v.visitor.VisitEachChild(node)
	frame := v.frames[index]
	if frame.err != nil {
		return expressionTransformResult{}, frame.err
	}
	return expressionTransformResult{expression: transformed, prerequisites: frame.prerequisites, imports: frame.imports}, nil
}

func (v *flameworkExpressionChildVisitor) visit(child *ast.Node) *ast.Node {
	index := len(v.frames) - 1
	if v.frames[index].err != nil {
		return child
	}
	runtime := v.frames[index].runtime
	transformed, err := transformFlameworkExpressionWithRuntime(v.state, child, runtime)
	if err != nil {
		if captureFlameworkDiagnostic(v.state, err) {
			return child
		}
		v.frames[index].err = err
		return child
	}
	// Recursion (including a custom runtime's reentry) can grow the stack and
	// replace its backing array. Reacquire the suspended frame by index.
	frame := &v.frames[index]
	frame.prerequisites = append(frame.prerequisites, transformed.prerequisites...)
	frame.imports = append(frame.imports, transformed.imports...)
	return transformed.expression
}

func transformFlameworkExpressionsInSourceFile(state *TransformState, sourceFile *ast.SourceFile) (*ast.SourceFile, error) {
	return transformFlameworkExpressionsInSourceFileWithRuntime(state, sourceFile, defaultFlameworkMacroRuntime())
}

func transformFlameworkExpressionsInSourceFileWithRuntime(state *TransformState, sourceFile *ast.SourceFile, runtime MacroRuntime) (*ast.SourceFile, error) {
	if sourceFile == nil {
		return nil, fmt.Errorf("%w: expression source file is nil", ErrInvalidTransformInput)
	}
	sourceFile, err := expandAddPaths(state, sourceFile)
	if err != nil {
		return nil, err
	}
	var transformErr error
	attributeSetterRequired := false
	imports := make([]MacroImport, 0)
	prerequisiteStack := make([][]*ast.Node, 0)
	var visitor *ast.NodeVisitor
	visitor = ast.NewNodeVisitor(func(node *ast.Node) *ast.Node {
		if transformErr != nil {
			return node
		}
		if ast.IsDecorator(node) {
			return node
		}
		if ast.IsStatement(node) {
			prerequisiteStack = append(prerequisiteStack, nil)
			transformed := visitor.VisitEachChild(node)
			prerequisites := prerequisiteStack[len(prerequisiteStack)-1]
			prerequisiteStack = prerequisiteStack[:len(prerequisiteStack)-1]
			if len(prerequisites) == 0 {
				return transformed
			}
			return state.factory.NewSyntaxList(append(prerequisites, transformed))
		}
		if ast.IsExpression(node) {
			transformed, err := transformFlameworkExpressionWithRuntime(state, node, runtime)
			if err != nil {
				if captureFlameworkDiagnostic(state, err) {
					return node
				}
				transformErr = err
				return node
			}
			if len(transformed.prerequisites) > 0 {
				if len(prerequisiteStack) == 0 {
					transformErr = fmt.Errorf("%w: macro prerequisites have no owning statement", ErrInvalidTransformInput)
					return node
				}
				index := len(prerequisiteStack) - 1
				prerequisiteStack[index] = append(prerequisiteStack[index], transformed.prerequisites...)
			}
			imports = append(imports, transformed.imports...)
			if transformed.expression != node && expressionUsesAttributeSetter(transformed.expression) {
				attributeSetterRequired = true
			}
			return transformed.expression
		}
		return visitor.VisitEachChild(node)
	}, state.factory, ast.NodeVisitorHooks{})
	transformed := visitor.VisitSourceFile(sourceFile)
	if transformErr != nil {
		return nil, transformErr
	}
	if !attributeSetterRequired && len(imports) == 0 {
		return transformed, nil
	}
	statements := macroImportStatements(state.factory, imports)
	if attributeSetterRequired {
		statements = append(statements, newAttributeSetterImport(state.factory))
	}
	statements = append(statements, transformed.Statements.Nodes...)
	return state.factory.UpdateSourceFile(
		transformed,
		state.factory.NewNodeList(statements),
		transformed.EndOfFileToken,
	).AsSourceFile(), nil
}

func defaultFlameworkMacroRuntime() MacroRuntime {
	return MacroRuntime{UUID: NewUUIDv4, RandomIndex: flameworkMacroRandomIndex, BuildGuard: buildFlameworkGuardForMacro}
}

func flameworkMacroRandomIndex(upperBound int) (int, error) {
	if upperBound <= 0 {
		return 0, fmt.Errorf("%w: upper bound %d must be positive", ErrInvalidMacroRandomIndex, upperBound)
	}
	value, err := rand.Int(rand.Reader, big.NewInt(int64(upperBound)))
	if err != nil {
		return 0, fmt.Errorf("generate Flamework macro random index: %w", err)
	}
	return int(value.Int64()), nil
}

func macroImportStatements(factory *ast.NodeFactory, imports []MacroImport) []*ast.Node {
	seen := make(map[MacroImport]struct{}, len(imports))
	statements := make([]*ast.Node, 0, len(imports))
	for _, macroImport := range imports {
		if _, ok := seen[macroImport]; ok {
			continue
		}
		seen[macroImport] = struct{}{}
		name := factory.NewIdentifier(macroImport.Local)
		var propertyName *ast.Node
		if macroImport.Export != macroImport.Local {
			propertyName = factory.NewIdentifier(macroImport.Export)
		}
		specifier := factory.NewImportSpecifier(false, propertyName, name)
		namedImports := factory.NewNamedImports(factory.NewNodeList([]*ast.Node{specifier}))
		clause := factory.NewImportClause(ast.KindUnknown, nil, namedImports)
		statements = append(statements, factory.NewImportDeclaration(nil, clause, factory.NewStringLiteral(macroImport.Module, ast.TokenFlagsNone), nil))
	}
	return statements
}

func macroTransformChanged(node, expression *ast.Node) bool {
	if expression == nil || expression.Kind != node.Kind || expression.Expression() != node.Expression() {
		return true
	}
	arguments := node.Arguments()
	transformedArguments := expression.Arguments()
	if len(arguments) != len(transformedArguments) {
		return true
	}
	for index := range arguments {
		if arguments[index] != transformedArguments[index] {
			return true
		}
	}
	return false
}

func macroTransformImports(state *TransformState, node *ast.Node, result MacroTransformResult) []MacroImport {
	imports := append([]MacroImport(nil), result.Imports...)
	guardReferences := synthesizedGuardReferenceCount(result.Expression)
	for _, prerequisite := range result.Prerequisites {
		guardReferences += synthesizedGuardReferenceCount(prerequisite)
	}
	if guardReferences > 0 {
		module := ""
		for range guardReferences {
			module = flameworkGuardLibrary(state, ast.GetSourceFileOfNode(node))
		}
		imports = append(imports, MacroImport{Module: module, Export: "t", Local: "t"})
	}
	return imports
}

func synthesizedGuardReferenceCount(root *ast.Node) int {
	if root == nil {
		return 0
	}
	count := 0
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsIdentifier(node) && node.Text() == "t" && node.Pos() < 0 {
			count++
		}
		node.ForEachChild(visit)
		return false
	}
	visit(root)
	return count
}
