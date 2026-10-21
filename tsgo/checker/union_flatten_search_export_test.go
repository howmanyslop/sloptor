package checker

// Hooks for union_flatten_search_test.go. That file is in package checker_test so that it
// can build real programs, and the union flatten helpers are unexported.

// AddTypesToUnionForTest adds types to a copy of typeSet with the capacity that
// getUnionTypeWorker gives a new set. It runs addTypesToUnion or, when baseline is set,
// baselineAddTypesToUnion.
func AddTypesToUnionForTest(c *Checker, typeSet []*Type, includes TypeFlags, types []*Type, baseline bool) ([]*Type, TypeFlags) {
	typeSet = append(make([]*Type, 0, len(typeSet)+len(types)), typeSet...)
	if baseline {
		return c.baselineAddTypesToUnion(typeSet, includes, types)
	}
	return c.addTypesToUnion(typeSet, includes, types)
}

// FreshLiteralTypeForTest returns the fresh type of a literal type.
func FreshLiteralTypeForTest(c *Checker, t *Type) *Type {
	return c.getFreshTypeOfLiteralType(t)
}

// baselineAddTypesToUnion is addTypesToUnion before the plain string literal fast path,
// unchanged: every constituent of a flattened union takes the full search in addTypeToUnion.
func (c *Checker) baselineAddTypesToUnion(typeSet []*Type, includes TypeFlags, types []*Type) ([]*Type, TypeFlags) {
	var lastType *Type
	for _, t := range types {
		if t != lastType {
			if t.flags&TypeFlagsUnion != 0 {
				u := t.AsUnionType()
				if t.alias != nil || u.origin != nil {
					includes |= TypeFlagsUnion
				}
				typeSet, includes = c.baselineAddTypesToUnion(typeSet, includes, u.types)
			} else {
				typeSet, includes = c.addTypeToUnion(typeSet, includes, t)
			}
			lastType = t
		}
	}
	return typeSet, includes
}
