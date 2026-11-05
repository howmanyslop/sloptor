// checkVariableHoist fixture (statements_test.go).
// - `shared` is declared directly in case 0 and written+read from case 1 (a
//   sibling clause; the write avoids TS2454 used-before-assigned) -> must
//   hoist, keyed on case 0's CaseClause.
// - `onlyMine` is declared and used only within case 2 -> no hoist decision.
// - `fallback` is declared in the DEFAULT clause -> checkVariableHoist only
//   handles CaseClause parents, so no decision either.
function describe(n: number): number {
	let result = 0;
	switch (n) {
		case 0:
			let shared = n + 1;
			result = shared;
			break;
		case 1:
			shared = n + 2;
			result = shared;
			break;
		case 2:
			const onlyMine = n * 2;
			result = onlyMine;
			break;
		default:
			const fallback = n - 1;
			result = fallback;
	}
	return result;
}
