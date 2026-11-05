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

export function formatRow(cells: string[], widths: number[]): string {
	let result = "";
	for (let i = 0; i < cells.length; i++) {
		const width = i < widths.length ? widths[i] : 0;
		if (i > 0) result += " | ";
		result += padLeft(cells[i], width, " ");
	}
	return result;
}

export function replicate(value: string, count: number): string {
	let result = "";
	for (let i = 0; i < count; i++) {
		result += value;
	}
	return result;
}
