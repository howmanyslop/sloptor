package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/checker"
)

type spreadDestructor func(s *State, parentID luau.AnyIdentifier, index int, state *bindingState) luau.Expression

func getSpreadDestructorForType(s *State, t *checker.Type, state *bindingState) spreadDestructor {
	if IsDefinitelyType(s, t, IsArrayType(s)) {
		return spreadDestructureArray
	}
	if state.step != nil {
		return spreadDestructureIterator
	}
	return nil
}

func spreadDestructureArray(s *State, parentID luau.AnyIdentifier, index int, state *bindingState) luau.Expression {
	return luau.NewCall(luau.GlobalProperty("table", "move"), luau.NewList[luau.Expression](
		parentID,
		luau.Num(float64(index+1)),
		luau.NewUnary("#", parentID),
		luau.Num(1),
		luau.NewArray(luau.NewList[luau.Expression]()),
	))
}

func spreadDestructureIterator(s *State, parentID luau.AnyIdentifier, index int, state *bindingState) luau.Expression {
	rest := s.PushToVar(luau.NewArray(luau.NewList[luau.Expression]()), "rest")
	length := s.PushToVar(luau.Num(0), "length")
	body := s.CaptureStatements(func() {
		value, done := state.step(s, false)
		s.Prereq(luau.NewAssignment(state.done, "=", done))
		s.Prereq(luau.NewIf(state.done, luau.NewList[luau.Statement](luau.NewBreak()), nil))
		s.Prereq(luau.NewAssignment(length, "+=", luau.Num(1)))
		s.Prereq(luau.NewAssignment(luau.NewComputedIndex(rest, length), "=", value))
	})
	s.Prereq(luau.NewWhile(luau.NewUnary("not", state.done), body))
	return rest
}

func spreadDestructureObject(s *State, parentID luau.AnyIdentifier, preSpreadNames []luau.Expression) luau.Expression {
	extractedMembers := luau.NewList[luau.Expression]()
	for _, expression := range preSpreadNames {
		switch expression := expression.(type) {
		case *luau.PropertyAccessExpression:
			extractedMembers.Push(luau.Str(expression.Name))
		case *luau.ComputedIndexExpression:
			extractedMembers.Push(expression.Index)
		default:
			panic("transformer: spreadDestructureObject: unknown expression type")
		}
	}
	extracted := s.PushToVar(luau.NewSet(extractedMembers), "extracted")
	rest := s.PushToVar(luau.NewMap(luau.NewList[*luau.MapField]()), "rest")
	keyID := luau.TempID("k")
	valueID := luau.TempID("v")
	s.Prereq(luau.NewFor(
		luau.NewList[luau.AnyIdentifier](keyID, valueID),
		parentID,
		luau.NewList[luau.Statement](
			luau.NewIf(
				luau.NewUnary("not", luau.NewComputedIndex(extracted, keyID)),
				luau.NewList[luau.Statement](
					luau.NewAssignment(luau.NewComputedIndex(rest, keyID), "=", valueID),
				),
				nil,
			),
		),
	))
	return rest
}
