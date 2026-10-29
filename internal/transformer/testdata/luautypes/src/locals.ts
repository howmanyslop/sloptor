// Placeholder for WP6: an explicitly annotated `let`, an uninitialized `let`
// and an inferred `const`.
export function locals(): number {
	let count: number = 0;
	let label: string | undefined;
	const names = ["a", "b"];
	for (const name of names) {
		count += name.size();
		label = name;
	}
	return label === undefined ? count : count + label.size();
}
