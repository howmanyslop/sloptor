export function gcd(left: number, right: number): number {
	let a = math.abs(left);
	let b = math.abs(right);
	while (b !== 0) {
		const remainder = a % b;
		a = b;
		b = remainder;
	}
	return a;
}

export function lcm(left: number, right: number): number {
	return left === 0 || right === 0 ? 0 : math.abs(left * right) / gcd(left, right);
}

export function extendedGcd(left: number, right: number): [number, number, number] {
	if (right === 0) return [left, 1, 0];
	const [divisor, x, y] = extendedGcd(right, left % right);
	return [divisor, y, x - math.floor(left / right) * y];
}

export function modularInverse(value: number, modulus: number): number | undefined {
	const [divisor, x] = extendedGcd(((value % modulus) + modulus) % modulus, modulus);
	return divisor !== 1 ? undefined : ((x % modulus) + modulus) % modulus;
}

export function modularPower(base: number, exponent: number, modulus: number): number {
	let result = 1;
	let current = base % modulus;
	let remaining = exponent;
	while (remaining > 0) {
		if (remaining % 2 === 1) result = (result * current) % modulus;
		current = (current * current) % modulus;
		remaining = math.floor(remaining / 2);
	}
	return result;
}

export function sieve(limit: number): Array<number> {
	const composite = new Set<number>();
	const primes = new Array<number>();
	for (let candidate = 2; candidate <= limit; candidate++) {
		if (composite.has(candidate)) continue;
		primes.push(candidate);
		for (let multiple = candidate * candidate; multiple <= limit; multiple += candidate) composite.add(multiple);
	}
	return primes;
}

export function isPrime(value: number): boolean {
	if (value < 2) return false;
	if (value % 2 === 0) return value === 2;
	for (let divisor = 3; divisor * divisor <= value; divisor += 2) if (value % divisor === 0) return false;
	return true;
}

export function factorize(value: number): Map<number, number> {
	const factors = new Map<number, number>();
	let remaining = value;
	for (let divisor = 2; divisor * divisor <= remaining; divisor++) {
		while (remaining % divisor === 0) {
			factors.set(divisor, (factors.get(divisor) ?? 0) + 1);
			remaining /= divisor;
		}
	}
	if (remaining > 1) factors.set(remaining, (factors.get(remaining) ?? 0) + 1);
	return factors;
}

export function eulerTotient(value: number): number {
	let result = value;
	for (const [prime] of factorize(value)) result = (result / prime) * (prime - 1);
	return result;
}

export function divisorCount(value: number): number {
	let count = 1;
	for (const [, exponent] of factorize(value)) count *= exponent + 1;
	return count;
}

export function chineseRemainder(remainders: ReadonlyArray<number>, moduli: ReadonlyArray<number>): number | undefined {
	let result = 0;
	let product = 1;
	for (const modulus of moduli) product *= modulus;
	for (let index = 0; index < moduli.size(); index++) {
		const partial = product / moduli[index];
		const inverse = modularInverse(partial, moduli[index]);
		if (inverse === undefined) return undefined;
		result = (result + remainders[index] * partial * inverse) % product;
	}
	return result;
}

export function binomial(n: number, k: number): number {
	if (k < 0 || k > n) return 0;
	let result = 1;
	for (let index = 1; index <= math.min(k, n - k); index++) result = (result * (n - index + 1)) / index;
	return math.floor(result + 0.5);
}

export function pascalRow(n: number): Array<number> {
	const row = [1];
	for (let k = 1; k <= n; k++) row.push((row[k - 1] * (n - k + 1)) / k);
	return row;
}

export function collatzLength(start: number, cache: Map<number, number>): number {
	if (start === 1) return 1;
	const cached = cache.get(start);
	if (cached !== undefined) return cached;
	const following = start % 2 === 0 ? start / 2 : 3 * start + 1;
	const length = collatzLength(following, cache) + 1;
	cache.set(start, length);
	return length;
}
