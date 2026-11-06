const WORD_BITS = 32;

export class BitSet {
	private readonly words: Array<number>;

	public constructor(private readonly capacity: number) {
		this.words = [];
		for (let index = 0; index < math.ceil(capacity / WORD_BITS); index++) this.words.push(0);
	}

	public static fromIndices(capacity: number, indices: ReadonlyArray<number>): BitSet {
		const set = new BitSet(capacity);
		for (const index of indices) set.set(index);
		return set;
	}

	public getCapacity(): number {
		return this.capacity;
	}

	private locate(bit: number): [number, number] {
		assert(bit >= 0 && bit < this.capacity, "bit out of range");
		return [math.floor(bit / WORD_BITS), bit % WORD_BITS];
	}

	public set(bit: number): void {
		const [word, offset] = this.locate(bit);
		this.words[word] = this.words[word] | (1 << offset);
	}

	public unset(bit: number): void {
		const [word, offset] = this.locate(bit);
		this.words[word] = this.words[word] & ~(1 << offset);
	}

	public toggle(bit: number): void {
		const [word, offset] = this.locate(bit);
		this.words[word] = this.words[word] ^ (1 << offset);
	}

	public has(bit: number): boolean {
		const [word, offset] = this.locate(bit);
		return ((this.words[word] >>> offset) & 1) === 1;
	}

	public cardinality(): number {
		let total = 0;
		for (const word of this.words) {
			let value = word;
			while (value !== 0) {
				value = value & (value - 1);
				total++;
			}
		}
		return total;
	}

	private combine(other: BitSet, operator: (left: number, right: number) => number): BitSet {
		const result = new BitSet(math.max(this.capacity, other.capacity));
		for (let index = 0; index < result.words.size(); index++) {
			result.words[index] = operator(this.words[index] ?? 0, other.words[index] ?? 0);
		}
		return result;
	}

	public union(other: BitSet): BitSet {
		return this.combine(other, (left, right) => left | right);
	}

	public intersection(other: BitSet): BitSet {
		return this.combine(other, (left, right) => left & right);
	}

	public difference(other: BitSet): BitSet {
		return this.combine(other, (left, right) => left & ~right);
	}

	public symmetricDifference(other: BitSet): BitSet {
		return this.combine(other, (left, right) => left ^ right);
	}

	public equals(other: BitSet): boolean {
		return this.symmetricDifference(other).cardinality() === 0;
	}

	public *indices(): Generator<number> {
		for (let bit = 0; bit < this.capacity; bit++) if (this.has(bit)) yield bit;
	}

	public nextSetBit(from: number): number {
		for (let bit = from; bit < this.capacity; bit++) if (this.has(bit)) return bit;
		return -1;
	}

	public toString(): string {
		let output = "";
		for (let bit = 0; bit < this.capacity; bit++) output += this.has(bit) ? "1" : "0";
		return output;
	}
}

export function primeBits(limit: number): BitSet {
	const composite = new BitSet(limit + 1);
	composite.set(0);
	composite.set(1);
	for (let candidate = 2; candidate * candidate <= limit; candidate++) {
		if (composite.has(candidate)) continue;
		for (let multiple = candidate * candidate; multiple <= limit; multiple += candidate) composite.set(multiple);
	}
	const primes = new BitSet(limit + 1);
	for (let bit = 0; bit <= limit; bit++) if (!composite.has(bit)) primes.set(bit);
	return primes;
}
