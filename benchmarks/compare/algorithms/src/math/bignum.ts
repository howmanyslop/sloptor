const LIMB = 10000;
const LIMB_DIGITS = 4;

export class BigNatural {
	private constructor(private readonly limbs: ReadonlyArray<number>) {}

	public static fromNumber(value: number): BigNatural {
		const limbs = new Array<number>();
		let remaining = math.floor(math.abs(value));
		while (remaining > 0) {
			limbs.push(remaining % LIMB);
			remaining = math.floor(remaining / LIMB);
		}
		return new BigNatural(limbs);
	}

	public static fromString(text: string): BigNatural {
		const limbs = new Array<number>();
		for (let finish = text.size(); finish >= 1; finish -= LIMB_DIGITS) {
			const start = math.max(1, finish - LIMB_DIGITS + 1);
			limbs.push(tonumber(text.sub(start, finish)) ?? 0);
		}
		return new BigNatural(BigNatural.trim(limbs));
	}

	private static trim(limbs: Array<number>): Array<number> {
		while (limbs.size() > 0 && limbs[limbs.size() - 1] === 0) limbs.pop();
		return limbs;
	}

	public isZero(): boolean {
		return this.limbs.size() === 0;
	}

	public compare(other: BigNatural): number {
		if (this.limbs.size() !== other.limbs.size()) return this.limbs.size() - other.limbs.size();
		for (let index = this.limbs.size() - 1; index >= 0; index--) {
			if (this.limbs[index] !== other.limbs[index]) return this.limbs[index] - other.limbs[index];
		}
		return 0;
	}

	public add(other: BigNatural): BigNatural {
		const result = new Array<number>();
		let carry = 0;
		for (let index = 0; index < math.max(this.limbs.size(), other.limbs.size()) || carry > 0; index++) {
			const sum = (this.limbs[index] ?? 0) + (other.limbs[index] ?? 0) + carry;
			result.push(sum % LIMB);
			carry = math.floor(sum / LIMB);
		}
		return new BigNatural(result);
	}

	public subtract(other: BigNatural): BigNatural {
		assert(this.compare(other) >= 0, "negative result");
		const result = new Array<number>();
		let borrow = 0;
		for (let index = 0; index < this.limbs.size(); index++) {
			let difference = this.limbs[index] - (other.limbs[index] ?? 0) - borrow;
			borrow = difference < 0 ? 1 : 0;
			if (difference < 0) difference += LIMB;
			result.push(difference);
		}
		return new BigNatural(BigNatural.trim(result));
	}

	public multiply(other: BigNatural): BigNatural {
		if (this.isZero() || other.isZero()) return new BigNatural([]);
		const result = new Array<number>();
		for (let index = 0; index < this.limbs.size() + other.limbs.size(); index++) result.push(0);
		for (let i = 0; i < this.limbs.size(); i++) {
			let carry = 0;
			for (let j = 0; j < other.limbs.size() || carry > 0; j++) {
				const current = result[i + j] + this.limbs[i] * (other.limbs[j] ?? 0) + carry;
				result[i + j] = current % LIMB;
				carry = math.floor(current / LIMB);
			}
		}
		return new BigNatural(BigNatural.trim(result));
	}

	public multiplySmall(factor: number): BigNatural {
		return this.multiply(BigNatural.fromNumber(factor));
	}

	public divideSmall(divisor: number): [BigNatural, number] {
		const result = new Array<number>();
		for (let index = 0; index < this.limbs.size(); index++) result.push(0);
		let remainder = 0;
		for (let index = this.limbs.size() - 1; index >= 0; index--) {
			const current = remainder * LIMB + this.limbs[index];
			result[index] = math.floor(current / divisor);
			remainder = current % divisor;
		}
		return [new BigNatural(BigNatural.trim(result)), remainder];
	}

	public power(exponent: number): BigNatural {
		let result = BigNatural.fromNumber(1);
		let base: BigNatural = this;
		let remaining = exponent;
		while (remaining > 0) {
			if (remaining % 2 === 1) result = result.multiply(base);
			base = base.multiply(base);
			remaining = math.floor(remaining / 2);
		}
		return result;
	}

	public digitSum(): number {
		let total = 0;
		for (const limb of this.limbs) {
			let value = limb;
			while (value > 0) {
				total += value % 10;
				value = math.floor(value / 10);
			}
		}
		return total;
	}

	public toString(): string {
		if (this.isZero()) return "0";
		let output = tostring(this.limbs[this.limbs.size() - 1]);
		for (let index = this.limbs.size() - 2; index >= 0; index--) output += string.format("%04d", this.limbs[index]);
		return output;
	}
}

export function factorial(n: number): BigNatural {
	let result = BigNatural.fromNumber(1);
	for (let k = 2; k <= n; k++) result = result.multiplySmall(k);
	return result;
}

export function bigFibonacci(n: number): BigNatural {
	let previous = BigNatural.fromNumber(0);
	let current = BigNatural.fromNumber(1);
	for (let step = 0; step < n; step++) {
		const following = previous.add(current);
		previous = current;
		current = following;
	}
	return previous;
}
