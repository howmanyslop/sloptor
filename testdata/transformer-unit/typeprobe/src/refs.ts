// Reference-walker fixture (refwalker_test.go). Outer `counter` has exactly
// four references after its definition, in this source order:
//   1. write  (assignment LHS)
//   2. read   (assignment RHS)
//   3. shorthand property value ({ counter })
//   4. closure read (arrow body)
// The identically-named variable inside `shadow` is a different symbol; its
// references must never be reported for the outer definition (and vice
// versa). `lonely` has no references at all.
let counter = 0;
counter = counter + 1;
const holder = { counter };
const grab = () => counter;
function shadow(): number {
	let counter = 100;
	counter += 1;
	return counter;
}
let lonely = 0;
