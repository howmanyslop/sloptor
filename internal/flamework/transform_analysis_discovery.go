package flamework

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

type discoveredClassState struct {
	class        DiscoveredFlameworkClass
	plan         ClassPlan
	dependencies []string
	node         *ast.Node
}

func discoverSourceClasses(state *TransformState, file *ast.SourceFile) ([]discoveredClassState, error) {
	nodes := make([]*ast.Node, 0)
	collectClassDeclarations(file.AsNode(), &nodes)
	classes := make([]discoveredClassState, 0, len(nodes))
	for _, node := range nodes {
		class, found, err := discoverClass(state, node)
		if err != nil {
			return nil, err
		}
		if found {
			classes = append(classes, class)
		}
	}
	return classes, nil
}

func collectClassDeclarations(node *ast.Node, classes *[]*ast.Node) {
	if ast.IsClassDeclaration(node) {
		*classes = append(*classes, node)
	}
	for child := range node.IterChildren() {
		collectClassDeclarations(child, classes)
	}
}

func discoverClass(state *TransformState, node *ast.Node) (discoveredClassState, bool, error) {
	if node.Name() == nil {
		return discoveredClassState{}, false, nil
	}
	internalID, err := declarationInternalID(state, node)
	if err != nil {
		return discoveredClassState{}, false, err
	}
	metadata := collectNodeMetadata(state, node)
	decorators, err := discoverDecorators(state, node)
	if err != nil {
		return discoveredClassState{}, false, err
	}
	containsLegacyDecorator := len(decorators) > 0 || hasMemberFlameworkDecorator(state, node)
	buildSnapshot := state.project.BuildInfoSnapshot()
	buildClassID := internalID
	if persistedID, ok := buildSnapshot.Identifiers[internalID]; ok {
		buildClassID = persistedID
	}
	buildClass, buildFallback := buildClassByInternalID(buildSnapshot, buildClassID)
	if !containsLegacyDecorator && !metadata.Requested("reflect") && !buildFallback {
		return discoveredClassState{}, false, nil
	}
	if len(decorators) == 0 && buildFallback {
		decorators = make([]DecoratorPlan, len(buildClass.Decorators))
		for index, decorator := range buildClass.Decorators {
			decorators[index] = DecoratorPlan(decorator)
		}
	}
	typeIDs, dependencies, err := constructorTypeIDs(state, node)
	if err != nil {
		return discoveredClassState{}, false, err
	}
	plan := ClassPlan{InternalID: internalID, Decorators: decorators, containsLegacyDecorator: containsLegacyDecorator}
	if err := preloadClassIdentifier(state.project, plan, ast.GetSourceFileOfNode(node).FileName()); err != nil {
		return discoveredClassState{}, false, err
	}
	if !state.project.IsGame() && !buildFallback {
		if err := addPackageBuildClass(state.project, plan, ast.GetSourceFileOfNode(node).FileName()); err != nil {
			return discoveredClassState{}, false, err
		}
	}
	decoratorIDs := make([]string, len(decorators))
	for index, decorator := range decorators {
		decoratorIDs[index] = decorator.InternalID
	}
	return discoveredClassState{
		class:        DiscoveredFlameworkClass{InternalID: internalID, DecoratorIDs: decoratorIDs, ConstructorTypeIDs: typeIDs},
		plan:         plan,
		dependencies: dependencies, node: node,
	}, true, nil
}

func discoverDecorators(state *TransformState, node *ast.Node) ([]DecoratorPlan, error) {
	decorators := make([]DecoratorPlan, 0)
	for _, decorator := range node.Decorators() {
		expression := decorator.Expression()
		identifier := expression
		if ast.IsCallExpression(expression) {
			identifier = expression.Expression()
		}
		name, named := decoratorReferenceName(identifier)
		if !named || !isFlameworkDecoratorType(state, expression) {
			continue
		}
		declaration := declarationFromNode(state, identifier)
		if declaration == nil {
			continue
		}
		internalID, err := declarationInternalID(state, declaration)
		if err != nil {
			return nil, err
		}
		decorators = append(decorators, DecoratorPlan{Name: name, InternalID: internalID})
	}
	return decorators, nil
}

func hasMemberFlameworkDecorator(state *TransformState, node *ast.Node) bool {
	for _, member := range node.Members() {
		for _, decorator := range member.Decorators() {
			if isFlameworkDecoratorType(state, decorator.Expression()) {
				return true
			}
		}
	}
	return false
}

func isFlameworkDecoratorType(state *TransformState, node *ast.Node) bool {
	for _, property := range state.checker.GetPropertiesOfType(state.checker.GetTypeAtLocation(node)) {
		if property.Name == "_flamework_Decorator" {
			return true
		}
	}
	return false
}

func constructorTypeIDs(state *TransformState, node *ast.Node) ([]string, []string, error) {
	for _, member := range node.Members() {
		if !ast.IsConstructorDeclaration(member) {
			continue
		}
		ids := make([]string, 0, len(member.Parameters()))
		dependencies := make([]string, 0, len(member.Parameters()))
		for _, parameter := range member.Parameters() {
			target := parameter.Type()
			var id string
			targetType := state.checker.GetTypeAtLocation(parameter)
			var err error
			if target == nil {
				id, err = typeUID(state, targetType, parameter)
			} else {
				id, err = nodeUID(state, target)
				targetType = state.checker.GetTypeAtLocation(target)
			}
			if err != nil {
				return nil, nil, err
			}
			ids = append(ids, id)
			if declaration := declarationFromType(state, targetType); declaration != nil {
				dependency, dependencyErr := declarationInternalID(state, declaration)
				if dependencyErr != nil {
					return nil, nil, dependencyErr
				}
				dependencies = append(dependencies, dependency)
			}
		}
		return ids, dependencies, nil
	}
	return nil, nil, nil
}

func declarationFromType(state *TransformState, targetType *checker.Type) *ast.Node {
	symbol := targetType.Symbol()
	if symbol == nil {
		return nil
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	if symbol.ValueDeclaration != nil {
		return symbol.ValueDeclaration
	}
	if len(symbol.Declarations) > 0 {
		return symbol.Declarations[0]
	}
	return nil
}

func declarationInternalID(state *TransformState, declaration *ast.Node) (string, error) {
	name := declarationFullName(declaration)
	fileName := ast.GetSourceFileOfNode(declaration).FileName()
	if pathWithin(state.project.PathTranslator().RootDir, fileName) {
		output := state.project.PathTranslator().GetOutputPath(fileName)
		relative, err := filepath.Rel(state.project.RootDirectory(), output)
		if err != nil {
			return "", fmt.Errorf("resolve Flamework declaration path: %w", err)
		}
		return state.project.PackageName() + ":" + strings.TrimSuffix(filepath.ToSlash(relative), filepath.Ext(relative)) + "@" + name, nil
	}
	packageJSON := findPackageJSON(filepath.Dir(fileName))
	data, err := os.ReadFile(packageJSON)
	if err != nil {
		return "", fmt.Errorf("read Flamework declaration package: %w", err)
	}
	var identity struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(data, &identity); err != nil || identity.Name == "" {
		return "", fmt.Errorf("%w: declaration package", ErrInvalidPackage)
	}
	relative, err := filepath.Rel(filepath.Dir(packageJSON), fileName)
	if err != nil {
		return "", fmt.Errorf("resolve packaged Flamework declaration path: %w", err)
	}
	relative = strings.TrimSuffix(strings.TrimSuffix(filepath.ToSlash(relative), ".ts"), ".d")
	if strings.HasSuffix(relative, "/index") {
		relative = strings.TrimSuffix(relative, "/index") + "/init"
	}
	return identity.Name + ":" + relative + "@" + name, nil
}
