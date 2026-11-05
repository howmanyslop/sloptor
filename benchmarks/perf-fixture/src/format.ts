// String macros (slice, toUpperCase, join, …) are not implemented by the Luau
// transform, so this file stays to primitives that are: length, concatenation,
// comparison and loops. That keeps the whole corpus buildable by sloptor.

export function padLeft(value: string, width: number, fill: string): string {
	let result = value;
	while (result.length < width) {
		result = fill + result;
	}
	return result;
}

export function formatRow(cells: Array<string>, widths: Array<number>): string {
	let result = "";
	// oxlint-disable-next-line unicorn-js/no-for-loop -- no
	for (let index = 0; index < cells.length; index += 1) {
		const width = index < widths.length ? widths[index] : 0;
		if (index > 0) result += " | ";
		result += padLeft(cells[index], width, " ");
	}
	return result;
}

export function replicate(value: string, count: number): string {
	let result = "";
	for (let index = 0; index < count; index += 1) {
		result += value;
	}
	return result;
}
