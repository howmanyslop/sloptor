package transformer

import "rotor/internal/luau"

type bindingState struct {
	matcher  luau.AnyIdentifier
	cursor   luau.AnyIdentifier
	consumed luau.AnyIdentifier
	done     luau.AnyIdentifier
	step     func(s *State, omitted bool) (value, done luau.Expression)
}

func collectionBindingState(s *State, parentID luau.AnyIdentifier, isMap bool) *bindingState {
	state := &bindingState{
		cursor:   s.PushToVar(nil, "key"),
		consumed: s.PushToVar(luau.NewArray(luau.NewList[luau.Expression]()), "consumed"),
		done:     s.PushToVar(luau.Bool(false), "done"),
	}
	state.step = func(s *State, omitted bool) (luau.Expression, luau.Expression) {
		var member luau.AnyIdentifier
		var target luau.NodeOrList = state.cursor
		var value luau.Expression = state.cursor
		if isMap && !omitted {
			member = s.PushToVar(nil, "value")
			target = luau.NewList[luau.AnyIdentifier](state.cursor, member)
			value = luau.NewArray(luau.NewList[luau.Expression](state.cursor, member))
		}
		advance := func() luau.Statement {
			return luau.NewAssignment(target, "=", luau.NewCall(luau.GlobalID("next"),
				luau.NewList[luau.Expression](parentID, state.cursor)))
		}
		// A computed destination can delete the cursor; recover without repeating consumed keys.
		s.Prereq(luau.NewIf(luau.NewBinary(
			luau.NewBinary(state.cursor, "~=", luau.Nil()), "and",
			luau.NewBinary(luau.NewComputedIndex(parentID, state.cursor), "==", luau.Nil()),
		), luau.NewList[luau.Statement](luau.NewAssignment(state.cursor, "=", luau.Nil())), nil))
		s.Prereq(advance())
		s.Prereq(luau.NewWhile(luau.NewBinary(
			luau.NewBinary(state.cursor, "~=", luau.Nil()), "and",
			luau.NewComputedIndex(state.consumed, state.cursor),
		), luau.NewList[luau.Statement](advance())))
		return value, luau.NewBinary(state.cursor, "==", luau.Nil())
	}
	return state
}

func generatorBindingState(s *State, parentID luau.AnyIdentifier) *bindingState {
	next := s.PushToVar(luau.NewPropertyAccess(parentID, "next"), "next")
	return &bindingState{
		done: s.PushToVar(luau.Bool(false), "done"),
		step: func(s *State, omitted bool) (luau.Expression, luau.Expression) {
			result := s.PushToVar(luau.NewCall(next, luau.NewList[luau.Expression]()), "result")
			return luau.NewPropertyAccess(result, "value"), luau.NewPropertyAccess(result, "done")
		},
	}
}

func iteratorBindingAccessor(s *State, parentID luau.AnyIdentifier, index int, state *bindingState, omitted bool) luau.Expression {
	var result luau.AnyIdentifier
	if !omitted {
		result = s.PushToVar(nil, "value")
	}
	body := s.CaptureStatements(func() {
		value, done := state.step(s, omitted)
		s.Prereq(luau.NewAssignment(state.done, "=", done))
		active := luau.NewList[luau.Statement]()
		if state.consumed != nil {
			active.Push(luau.NewAssignment(luau.NewComputedIndex(state.consumed, state.cursor), "=", luau.Bool(true)))
		}
		if result != nil {
			active.Push(luau.NewAssignment(result, "=", value))
		}
		if active.IsNonEmpty() {
			s.Prereq(luau.NewIf(luau.NewUnary("not", state.done), active, nil))
		}
	})
	s.Prereq(luau.NewIf(luau.NewUnary("not", state.done), body, nil))
	if result == nil {
		return luau.NewNone()
	}
	return result
}
