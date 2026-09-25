package transformer

import "rotor/tsgo/checker"

func containsInstantiableType(t *checker.Type) bool {
	if t.IsUnion() || t.IsIntersection() {
		for _, member := range t.Types() {
			if containsInstantiableType(member) {
				return true
			}
		}
		return false
	}
	return t.Flags()&checker.TypeFlagsInstantiable != 0
}

func canChangeReceiverConvention(s *State, t *checker.Type) bool {
	if !containsInstantiableType(t) {
		return false
	}
	if t.IsUnion() {
		for _, member := range t.Types() {
			if member.Flags()&(checker.TypeFlagsVoid|checker.TypeFlagsNever) == 0 && !containsInstantiableType(member) {
				return false
			}
		}
	}
	if t.Flags()&checker.TypeFlagsConditional != 0 {
		conditional := t.AsConditionalType()
		checkType, extendsType := conditional.RootCheckType(), conditional.ExtendsType()
		if checkType.IsTupleType() && extendsType.IsTupleType() {
			checkElements, extendsElements := getTypeArguments(s, checkType), getTypeArguments(s, extendsType)
			if len(checkElements) == 1 && len(extendsElements) == 1 {
				checkType, extendsType = checkElements[0], extendsElements[0]
			}
		}
		root := conditional.RootNode()
		removesVoid := s.Checker.GetTypeFromTypeNode(root.TrueType).Flags()&checker.TypeFlagsNever != 0 &&
			s.Checker.GetTypeFromTypeNode(root.FalseType) == checkType &&
			s.Checker.IsTypeAssignableTo(s.Checker.GetVoidType(), extendsType)
		if removesVoid {
			return false
		}
	}
	constraint := s.Checker.GetBaseConstraintOfType(t)
	return constraint == nil || s.Checker.IsTypeAssignableTo(s.Checker.GetVoidType(), constraint)
}
