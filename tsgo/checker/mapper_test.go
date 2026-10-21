package checker

import (
	"slices"
	"testing"
)

func TestMapWithCombinedTypeMappers_dispatch(t *testing.T) {
	input := &Type{}
	changed := &Type{}
	output := &Type{}
	tests := []struct {
		name      string
		first     *Type
		nilFirst  bool
		nilSecond bool
		want      *Type
		wantCalls []string
		wantPanic bool
	}{
		{
			name:      "nil first mapper",
			nilFirst:  true,
			want:      output,
			wantCalls: []string{"second"},
		},
		{
			name:      "identity first mapper",
			first:     input,
			want:      output,
			wantCalls: []string{"first", "second"},
		},
		{
			name:      "changed type with nil second mapper",
			first:     changed,
			nilSecond: true,
			want:      changed,
			wantCalls: []string{"first"},
		},
		{
			name:      "changed to nil type",
			wantCalls: []string{"first"},
		},
		{
			name:      "both mappers nil",
			nilFirst:  true,
			nilSecond: true,
			wantPanic: true,
		},
		{
			name:      "identity with nil second mapper",
			first:     input,
			nilSecond: true,
			wantCalls: []string{"first"},
			wantPanic: true,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// These paths must not need checker state, including instantiateType's
			// nil mapper/type short circuits. Generic instantiation needs a real program.
			var c *Checker
			for _, useHelper := range []bool{false, true} {
				var calls []string
				var m1, m2 *TypeMapper
				if !test.nilFirst {
					m1 = newFunctionTypeMapper(func(got *Type) *Type {
						calls = append(calls, "first")
						if got != input {
							t.Fatalf("helper=%v: first input = %p, want %p", useHelper, got, input)
						}
						return test.first
					})
				}
				if !test.nilSecond {
					m2 = newFunctionTypeMapper(func(got *Type) *Type {
						calls = append(calls, "second")
						if got != input {
							t.Fatalf("helper=%v: second input = %p, want %p", useHelper, got, input)
						}
						return output
					})
				}
				var got *Type
				var panicValue any
				func() {
					defer func() { panicValue = recover() }()
					if useHelper {
						got = c.mapWithCombinedTypeMappers(input, m1, m2)
					} else {
						got = c.combineTypeMappers(m1, m2).Map(input)
					}
				}()
				if (panicValue != nil) != test.wantPanic {
					t.Fatalf("helper=%v: panic = %v, want panic %v", useHelper, panicValue, test.wantPanic)
				}
				if got != test.want || !slices.Equal(calls, test.wantCalls) {
					t.Fatalf("helper=%v: result = %p, calls = %v; want %p, %v", useHelper, got, calls, test.want, test.wantCalls)
				}
			}
		})
	}
}

func TestMapWithCombinedTypeMappers_statefulBatch(t *testing.T) {
	inputs := []*Type{{}, {}, {}}
	targets := []*Type{{}, {}, {}}
	for _, useHelper := range []bool{false, true} {
		var c *Checker
		var calls []string
		firstCount, secondCount := 0, 0
		m1 := newDeferredTypeMapper(inputs, []func() *Type{
			func() *Type { calls = append(calls, "first0"); firstCount++; return inputs[0] },
			func() *Type { calls = append(calls, "first1"); firstCount++; return inputs[1] },
			func() *Type { calls = append(calls, "first2"); firstCount++; return inputs[2] },
		})
		m2 := newFunctionTypeMapper(func(got *Type) *Type {
			if secondCount >= len(inputs) || got != inputs[secondCount] || firstCount != secondCount+1 {
				t.Fatalf("helper=%v: out-of-order mapping: first=%d second=%d input=%p", useHelper, firstCount, secondCount, got)
			}
			calls = append(calls, "second")
			result := targets[secondCount]
			secondCount++
			return result
		})
		var mapType func(*Type) *Type
		if useHelper {
			mapType = func(input *Type) *Type { return c.mapWithCombinedTypeMappers(input, m1, m2) }
		} else {
			mapType = c.combineTypeMappers(m1, m2).Map
		}
		for i, input := range inputs {
			if got := mapType(input); got != targets[i] {
				t.Fatalf("helper=%v: result %d = %p, want %p", useHelper, i, got, targets[i])
			}
		}
		wantCalls := []string{"first0", "second", "first1", "second", "first2", "second"}
		if !slices.Equal(calls, wantCalls) {
			t.Fatalf("helper=%v: calls = %v, want %v", useHelper, calls, wantCalls)
		}
	}
}
