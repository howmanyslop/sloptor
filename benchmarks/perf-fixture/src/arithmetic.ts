// biome-ignore-all lint/style/noParameterAssign: no

export function clamp(value: number, min: number, max: number): number {
	if (value < min) return min;
	if (value > max) return max;
	return value;
}

export function fibonacci(n: number): number {
	let a = 0;
	let b = 1;
	for (let i = 0; i < n; i++) {
		const following = a + b;
		a = b;
		b = following;
	}
	return a;
}

export function gcd(a: number, b: number): number {
	while (b !== 0) {
		const remainder = a % b;
		a = b;
		b = remainder;
	}
	return a;
}

export function polynomial(x: number, coefficients: number[]): number {
	let result = 0;
	for (let i = 0; i < coefficients.length; i++) {
		result = result * x + coefficients[i];
	}
	return result;
}
