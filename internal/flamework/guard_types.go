package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

var robloxGuardTypes = []string{
	"UDim", "UDim2", "BrickColor", "Color3", "Vector2", "Vector3", "NumberSequence",
	"NumberSequenceKeypoint", "ColorSequence", "ColorSequenceKeypoint", "NumberRange", "Rect",
	"DockWidgetPluginGuiInfo", "CFrame", "Axes", "Faces", "Font", "Instance", "Ray", "Random",
	"Region3", "Region3int16", "Enum", "TweenInfo", "PhysicalProperties", "Vector3int16",
	"Vector2int16", "PathWaypoint", "EnumItem", "RBXScriptSignal", "RBXScriptConnection",
	"FloatCurveKey", "OverlapParams", "thread", "buffer",
}

func guardTypeArguments(typeValue *checker.Type, typeChecker *checker.Checker) []*checker.Type {
	if typeValue.Flags()&checker.TypeFlagsObject == 0 || typeValue.ObjectFlags()&checker.ObjectFlagsReference == 0 {
		return nil
	}
	return typeChecker.GetTypeArguments(typeValue)
}

func guardDeclaration(typeValue *checker.Type) *ast.Node {
	if symbol := typeValue.Symbol(); symbol != nil && len(symbol.Declarations) > 0 {
		return symbol.Declarations[0]
	}
	return nil
}

func guardIsDefinedType(typeValue *checker.Type, typeChecker *checker.Checker) bool {
	return typeValue.Flags() == checker.TypeFlagsObject &&
		len(typeChecker.GetPropertiesOfType(typeValue)) == 0 &&
		len(typeChecker.GetCallSignatures(typeValue)) == 0 &&
		len(typeChecker.GetConstructSignatures(typeValue)) == 0 &&
		typeChecker.GetNumberIndexType(typeValue) == nil &&
		typeChecker.GetStringIndexType(typeValue) == nil
}

func (g *guardGenerator) buildUnion(typeValue *checker.Type) (*ast.Node, error) {
	if typeValue == g.state.checker.GetBooleanType() {
		return guardField(g.state.factory, "t", "boolean"), nil
	}
	optional := false
	guards := make([]*ast.Node, 0, len(typeValue.Types()))
	literals := make([]*ast.Node, 0)
	enumRoot := g.state.checker.ResolveName("Enum", nil, ast.SymbolFlagsType, false)
	type enumGroup struct {
		symbol  *ast.Symbol
		members []*checker.Type
	}
	enumGroups := make([]enumGroup, 0)
	for _, member := range typeValue.Types() {
		if member == g.state.checker.GetUndefinedType() || member == g.state.checker.GetVoidType() {
			optional = true
			continue
		}
		if member.Flags()&checker.TypeFlagsESSymbolLike != 0 {
			continue
		}
		literal, ok, err := g.literalValue(member)
		if err != nil {
			return nil, err
		}
		if ok {
			literals = append(literals, literal)
			continue
		}
		if enumKind := guardRobloxEnumKind(member, enumRoot, g.state.checker); enumKind != nil {
			index := -1
			for candidate := range enumGroups {
				if enumGroups[candidate].symbol == enumKind {
					index = candidate
					break
				}
			}
			if index == -1 {
				enumGroups = append(enumGroups, enumGroup{symbol: enumKind})
				index = len(enumGroups) - 1
			}
			enumGroups[index].members = append(enumGroups[index].members, member)
			continue
		}
		guard, err := g.build(member)
		if err != nil {
			return nil, err
		}
		guards = append(guards, guard)
	}
	for _, group := range enumGroups {
		if len(group.members)+1 == len(group.symbol.Exports) {
			enumExpression := guardField(g.state.factory, "Enum", group.symbol.Name)
			guards = append(guards, guardCall(g.state.factory, "enum", enumExpression))
			continue
		}
		for _, member := range group.members {
			enumExpression := guardField(g.state.factory, "Enum", group.symbol.Name)
			literals = append(literals, g.state.factory.NewElementAccessExpression(
				enumExpression,
				nil,
				g.state.factory.NewStringLiteral(member.Symbol().Name, ast.TokenFlagsNone),
				ast.NodeFlagsNone,
			))
		}
	}
	if len(literals) > 0 {
		guards = append(guards, guardListCall(g.state.factory, "literal", literals))
	}
	union := guardField(g.state.factory, "t", "none")
	if len(guards) == 1 {
		union = guards[0]
	} else if len(guards) > 1 {
		union = guardListCall(g.state.factory, "union", guards)
	}
	if optional && len(guards) > 0 {
		return guardCall(g.state.factory, "optional", union), nil
	}
	return union, nil
}

func guardRobloxEnumKind(typeValue *checker.Type, enumRoot *ast.Symbol, typeChecker *checker.Checker) *ast.Symbol {
	symbol := typeValue.Symbol()
	if symbol == nil || symbol.Parent == nil || symbol.Parent.Parent == nil || enumRoot == nil {
		return nil
	}
	enumKind := symbol.Parent
	if typeChecker.GetMergedSymbol(enumKind.Parent) != enumRoot || enumKind.Exports[symbol.Name] != symbol {
		return nil
	}
	return enumKind
}

func (g *guardGenerator) buildIntersection(typeValue *checker.Type) (*ast.Node, error) {
	if len(g.state.checker.GetIndexInfosOfType(typeValue)) > 1 {
		return nil, g.fail(typeValue, "intersections with multiple index signatures are unsupported")
	}
	for _, member := range typeValue.Types() {
		if member.Flags()&checker.TypeFlagsDisjointDomains != 0 {
			return g.build(member)
		}
	}
	guards, err := g.buildAll(typeValue.Types())
	if err != nil {
		return nil, err
	}
	return guardListCall(g.state.factory, "intersection", guards), nil
}

func (g *guardGenerator) buildObject(typeValue *checker.Type) (*ast.Node, error) {
	properties := g.state.checker.GetApparentProperties(typeValue)
	indexInfos := g.state.checker.GetIndexInfosOfType(typeValue)
	if len(properties) == 0 && len(indexInfos) == 0 {
		return guardField(g.state.factory, "t", "any"), nil
	}
	guards := make([]*ast.Node, 0, 2)
	if len(properties) > 0 {
		propertyGuards, err := g.buildProperties(typeValue, true)
		if err != nil {
			return nil, err
		}
		guards = append(guards, guardCall(g.state.factory, "interface", guardObject(g.state.factory, propertyGuards)))
	}
	if len(indexInfos) > 1 {
		return nil, g.fail(typeValue, "types with multiple index signatures are unsupported")
	}
	if len(indexInfos) == 1 {
		keyGuard, err := g.build(indexInfos[0].KeyType())
		if err != nil {
			return nil, err
		}
		valueGuard, err := g.build(indexInfos[0].ValueType())
		if err != nil {
			return nil, err
		}
		guards = append(guards, guardCall(g.state.factory, "map", keyGuard, valueGuard))
	}
	if len(guards) == 1 {
		return guards[0], nil
	}
	return guardListCall(g.state.factory, "intersection", guards), nil
}

func (g *guardGenerator) buildProperties(typeValue *checker.Type, interfaceType bool) ([]guardProperty, error) {
	properties := make([]guardProperty, 0)
	for _, property := range g.state.checker.GetPropertiesOfType(typeValue) {
		propertyType := g.state.checker.GetTypeOfPropertyOfType(typeValue, property.Name)
		if propertyType == nil {
			return nil, g.fail(typeValue, fmt.Sprintf("could not find type for field %q", property.Name))
		}
		if interfaceType && propertyType.Flags()&(checker.TypeFlagsUnknown|checker.TypeFlagsNever|checker.TypeFlagsUniqueESSymbol) != 0 {
			continue
		}
		if property.ValueDeclaration != nil {
			g.tracking = append(g.tracking, guardTracking{node: property.ValueDeclaration, typeValue: propertyType})
		}
		guard, err := g.build(propertyType)
		if property.ValueDeclaration != nil {
			g.tracking = g.tracking[:len(g.tracking)-1]
		}
		if err != nil {
			return nil, err
		}
		properties = append(properties, guardProperty{name: property.Name, guard: guard})
	}
	return properties, nil
}

func (g *guardGenerator) buildMap(typeValue *checker.Type) (*ast.Node, error) {
	arguments := guardTypeArguments(typeValue, g.state.checker)
	guards := []*ast.Node{guardField(g.state.factory, "t", "any"), guardField(g.state.factory, "t", "any")}
	for index := range min(len(arguments), 2) {
		guard, err := g.build(arguments[index])
		if err != nil {
			return nil, err
		}
		guards[index] = guard
	}
	return guardCall(g.state.factory, "map", guards...), nil
}

func (g *guardGenerator) buildSet(typeValue *checker.Type) (*ast.Node, error) {
	arguments := guardTypeArguments(typeValue, g.state.checker)
	guard := guardField(g.state.factory, "t", "any")
	if len(arguments) > 0 {
		var err error
		guard, err = g.build(arguments[0])
		if err != nil {
			return nil, err
		}
	}
	return guardCall(g.state.factory, "set", guard), nil
}
