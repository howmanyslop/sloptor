package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
)

func transformFlameworkClass(state *TransformState, plan FilePlan, node *ast.Node) (*ast.Node, error) {
	transformed, err := transformFlameworkClassWorker(state, plan, node)
	if err != nil && addClassTransformDiagnostic(state, node, err) {
		return node, nil
	}
	return transformed, err
}

func transformFlameworkClassWorker(state *TransformState, plan FilePlan, node *ast.Node) (*ast.Node, error) {
	semanticClass := originalClassNode(state, node)
	analysis, err := analyzeFlameworkClass(state, plan, semanticClass)
	if err != nil {
		return nil, err
	}
	class := node.AsClassDeclaration()
	classPlan, _ := plannedClassByName(plan, analysis.name)
	prerequisites := make([]*ast.Node, 0)
	imports := make([]MacroImport, 0)
	classDecoratorStatements := make([]*ast.Node, 0, len(analysis.decorators))
	for index := len(analysis.decorators) - 1; index >= 0; index-- {
		decorator := analysis.decorators[index]
		prerequisites = append(prerequisites, decorator.prerequisites...)
		imports = append(imports, decorator.imports...)
		statement, statementErr := newDecoratorStatement(state, analysis.name, decorator)
		if statementErr != nil {
			return nil, statementErr
		}
		classDecoratorStatements = append(classDecoratorStatements, statement)
	}
	staticStatements := make([]*ast.Node, 0)
	containsLegacyDecorator := analysis.containsLegacyDecorator || len(analysis.decorators) > 0 || hasMemberFlameworkDecorator(state, semanticClass)
	if containsLegacyDecorator || analysis.metadata.Requested("identifier") {
		staticStatements = append(staticStatements, newMetadataStatement(state.factory, metadataCall{
			analysis.name, "identifier", state.factory.NewStringLiteral(analysis.identifier, ast.TokenFlagsNone),
		}))
	}
	for _, member := range semanticClass.AsClassDeclaration().Members.Nodes {
		if !ast.IsConstructorDeclaration(member) {
			continue
		}
		constructorReflection, reflectionErr := constructorReflectionStatements(state, analysis.name, member)
		if reflectionErr != nil {
			return nil, reflectionErr
		}
		staticStatements = append(staticStatements, constructorReflection...)
		break
	}
	if analysis.metadata.Requested("flamework:implements") && len(analysis.implementedIDs) > 0 {
		staticStatements = append(staticStatements, newMetadataStatement(
			state.factory, metadataCall{analysis.name, "flamework:implements", stringArray(state.factory, analysis.implementedIDs)},
		))
	}
	reflectIdentifier := reflectIdentifierForNode(node)
	members := make([]*ast.Node, 0, len(class.Members.Nodes)+1)
	memberDecoratorStatements := make([]*ast.Node, 0)
	semanticMembers := semanticClass.AsClassDeclaration().Members.Nodes
	for index, member := range class.Members.Nodes {
		semanticMember := member
		if index < len(semanticMembers) {
			semanticMember = semanticMembers[index]
		}
		reflection, reflectionErr := memberReflectionStatements(state, memberReflectionInput{plan: plan, className: analysis.name, member: semanticMember})
		if reflectionErr != nil {
			return nil, reflectionErr
		}
		staticStatements = append(staticStatements, reflection...)
		updatedMember, statements, memberPrerequisites, memberImports, memberErr := transformFlameworkClassMember(
			state, decoratorTarget{className: analysis.name, plan: classPlan}, member, semanticMember,
		)
		if memberErr != nil {
			return nil, memberErr
		}
		members = append(members, updatedMember)
		memberDecoratorStatements = append(memberDecoratorStatements, statements...)
		prerequisites = append(prerequisites, memberPrerequisites...)
		imports = append(imports, memberImports...)
	}
	if len(staticStatements) > 0 {
		state.EmitContext().AddSyntheticLeadingComment(
			staticStatements[0], ast.KindSingleLineCommentTrivia, fmt.Sprintf(" (Flamework) %s metadata", analysis.name), true,
		)
	}
	if len(classDecoratorStatements) > 0 {
		state.EmitContext().AddSyntheticLeadingComment(
			classDecoratorStatements[0], ast.KindSingleLineCommentTrivia, fmt.Sprintf(" (Flamework) %s decorators", analysis.name), true,
		)
	} else if len(memberDecoratorStatements) > 0 {
		state.EmitContext().AddSyntheticLeadingComment(
			memberDecoratorStatements[0], ast.KindSingleLineCommentTrivia, fmt.Sprintf(" (Flamework) %s decorators", analysis.name), true,
		)
	}
	staticBlock := state.factory.NewClassStaticBlockDeclaration(
		nil,
		state.factory.NewBlock(state.factory.NewNodeList(rewriteReflectStatements(state.factory, staticStatements, reflectIdentifier)), true),
	)
	members = append(members, staticBlock)
	updatedClass := state.factory.UpdateClassDeclaration(
		class,
		stripFlameworkDecorators(state, class.Modifiers()),
		class.Name(),
		class.TypeParameters,
		class.HeritageClauses,
		state.factory.NewNodeList(members),
	)
	statements := macroImportStatements(state.factory, imports)
	statements = append(statements, prerequisites...)
	statements = append(statements, updatedClass)
	statements = append(statements, classDecoratorStatements...)
	statements = append(statements, memberDecoratorStatements...)
	return state.factory.NewSyntaxList(statements), nil
}

func originalClassNode(state *TransformState, node *ast.Node) *ast.Node {
	sourceFile := ast.GetSourceFileOfNode(node)
	original := state.program.GetSourceFile(sourceFile.FileName())
	if original == nil {
		return node
	}
	ast.SetParentInChildren(original.AsNode())
	var match *ast.Node
	var visit func(*ast.Node)
	visit = func(current *ast.Node) {
		if match != nil {
			return
		}
		if ast.IsClassDeclaration(current) && current.Pos() == node.Pos() && current.End() == node.End() {
			match = current
			return
		}
		for child := range current.IterChildren() {
			visit(child)
		}
	}
	visit(original.AsNode())
	if match != nil {
		return match
	}
	return node
}

func transformFlameworkClassMember(
	state *TransformState,
	target decoratorTarget,
	member *ast.Node,
	semanticMember *ast.Node,
) (*ast.Node, []*ast.Node, []*ast.Node, []MacroImport, error) {
	if semanticMember.Name() == nil {
		if len(semanticMember.Decorators()) > 0 {
			for _, decorator := range semanticMember.Decorators() {
				if isFlameworkDecorator(state, decorator) {
					return nil, nil, nil, nil, fmt.Errorf("%w: unsupported decorator placement on %s", ErrInvalidDecorator, member.Kind)
				}
			}
		}
		return member, nil, nil, nil, nil
	}
	decorators, err := analyzeNodeDecorators(state, target.plan, semanticMember)
	if err != nil {
		return nil, nil, nil, nil, err
	}
	updated := member
	switch {
	case ast.IsMethodDeclaration(member):
		method := member.AsMethodDeclaration()
		updated = state.factory.UpdateMethodDeclaration(
			method, stripFlameworkDecorators(state, semanticMember.Modifiers()), method.AsteriskToken, method.Name(), method.PostfixToken,
			method.TypeParameters, method.Parameters, method.Type, method.FullSignature, method.Body,
		)
	case ast.IsPropertyDeclaration(member):
		property := member.AsPropertyDeclaration()
		updated = state.factory.UpdatePropertyDeclaration(
			property, stripFlameworkDecorators(state, semanticMember.Modifiers()), property.Name(), property.PostfixToken, property.Type, property.Initializer,
		)
	case ast.IsGetAccessorDeclaration(member):
		accessor := member.AsGetAccessorDeclaration()
		updated = state.factory.UpdateGetAccessorDeclaration(
			accessor, stripFlameworkDecorators(state, semanticMember.Modifiers()), accessor.Name(), accessor.TypeParameters,
			accessor.Parameters, accessor.Type, accessor.FullSignature, accessor.Body,
		)
	case ast.IsSetAccessorDeclaration(member):
		accessor := member.AsSetAccessorDeclaration()
		updated = state.factory.UpdateSetAccessorDeclaration(
			accessor, stripFlameworkDecorators(state, semanticMember.Modifiers()), accessor.Name(), accessor.TypeParameters,
			accessor.Parameters, accessor.Type, accessor.FullSignature, accessor.Body,
		)
	}
	target.propertyName = ast.GetPropertyNameForPropertyNameNode(semanticMember.Name())
	target.static = ast.HasStaticModifier(semanticMember)
	target.member = true
	statements := make([]*ast.Node, 0, len(decorators))
	prerequisites := make([]*ast.Node, 0)
	imports := make([]MacroImport, 0)
	for index := len(decorators) - 1; index >= 0; index-- {
		decorator := decorators[index]
		prerequisites = append(prerequisites, decorator.prerequisites...)
		imports = append(imports, decorator.imports...)
		statement, statementErr := newDecoratorStatementForTarget(state, target, decorator)
		if statementErr != nil {
			return nil, nil, nil, nil, statementErr
		}
		statements = append(statements, statement)
	}
	return updated, statements, prerequisites, imports, nil
}

func rewriteReflectStatements(factory *ast.NodeFactory, statements []*ast.Node, identifier string) []*ast.Node {
	if identifier == "Reflect" {
		return statements
	}
	result := make([]*ast.Node, len(statements))
	for index, statement := range statements {
		var visitor *ast.NodeVisitor
		visitor = ast.NewNodeVisitor(func(node *ast.Node) *ast.Node {
			if ast.IsElementAccessExpression(node) && ast.IsIdentifier(node.Expression()) && node.Expression().Text() == "Reflect" {
				access := node.AsElementAccessExpression()
				return factory.UpdateElementAccessExpression(access, factory.NewIdentifier(identifier), access.QuestionDotToken, access.ArgumentExpression, access.Flags)
			}
			return visitor.VisitEachChild(node)
		}, factory, ast.NodeVisitorHooks{})
		result[index] = visitor.VisitNode(statement)
	}
	return result
}
