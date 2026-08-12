package flamework

import "rotor/tsgo/checker"

func guardTypesRequiringDedup(typeValue *checker.Type, typeChecker *checker.Checker, limit int) map[*checker.Type]bool {
	counts := make(map[*checker.Type]int)
	order := make([]*checker.Type, 0)
	visiting := make(map[*checker.Type]bool)
	var count func(*checker.Type, int)
	count = func(current *checker.Type, modifier int) {
		if current == nil {
			return
		}
		if current.Flags()&(checker.TypeFlagsObject|checker.TypeFlagsUnionOrIntersection) != 0 {
			if _, seen := counts[current]; !seen {
				order = append(order, current)
			}
			counts[current] += modifier
		}
		if visiting[current] {
			return
		}
		visiting[current] = true
		defer delete(visiting, current)
		if current.Flags()&checker.TypeFlagsUnionOrIntersection != 0 {
			for _, child := range current.Types() {
				count(child, modifier)
			}
			return
		}
		if current.Flags()&checker.TypeFlagsObject == 0 || typeChecker.GetPropertyOfType(current, "_nominal_Instance") != nil {
			return
		}
		for _, property := range typeChecker.GetPropertiesOfType(current) {
			count(typeChecker.GetTypeOfPropertyOfType(current, property.Name), modifier)
		}
		for _, indexInfo := range typeChecker.GetIndexInfosOfType(current) {
			count(indexInfo.KeyType(), modifier)
			count(indexInfo.ValueType(), modifier)
		}
	}
	count(typeValue, 1)
	required := make(map[*checker.Type]bool)
	for {
		var selected *checker.Type
		for _, current := range order {
			seen := counts[current]
			if seen < limit || required[current] {
				continue
			}
			selected = current
			break
		}
		if selected == nil {
			return required
		}
		required[selected] = true
		count(selected, -(counts[selected] - 1))
	}
}
