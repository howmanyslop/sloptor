// oxlint-disable no-param-reassign -- no
// biome-ignore-all lint/style/noParameterAssign: no

export function clamp(value: number, min: number, max: number): number {
	if (value < min) return min;
	if (value > max) return max;
	return value;
}

export function getFibonacci(number: number): number {
	let a = 0;
	let b = 1;
	for (let index = 0; index < number; index += 1) {
		const following = a + b;
		a = b;
		b = following;
	}
	return a;
}

export function getGcd(a: number, b: number): number {
	while (b !== 0) {
		const remainder = a % b;
		a = b;
		b = remainder;
	}
	return a;
}

export function getPolynomial(x: number, coefficients: Array<number>): number {
	let result = 0;
	for (const coefficient of coefficients) {
		result = result * x + coefficient;
	}
	return result;
}
