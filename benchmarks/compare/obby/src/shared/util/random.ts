import { add32, fnv1a, imul32, mixBits, toUint32 } from "./bits";

const UINT32_RANGE = 4294967296;

/**
 * Small deterministic generator in the mulberry32 / xorshift family. Every
 * operation goes through bit32 so results are identical on every server.
 */
export class SeededRandom {
	private static instanceCount = 0;

	public readonly seed: number;
	private state: number;
	private draws = 0;

	public constructor(seed: number) {
		this.seed = toUint32(seed);
		this.state = mixBits(this.seed === 0 ? 0x6d2b79f5 : this.seed);
		SeededRandom.instanceCount += 1;
	}

	public static fromString(label: string): SeededRandom {
		return new SeededRandom(fnv1a(label));
	}

	public static getInstanceCount(): number {
		return SeededRandom.instanceCount;
	}

	public getDrawCount(): number {
		return this.draws;
	}

	public nextUint32(): number {
		this.draws += 1;
		this.state = add32(this.state, 0x6d2b79f5);
		let value = this.state;
		value = imul32(bit32.bxor(value, bit32.rshift(value, 15)), bit32.bor(value, 1));
		value = bit32.bxor(value, add32(value, imul32(bit32.bxor(value, bit32.rshift(value, 7)), bit32.bor(value, 61))));
		return bit32.bxor(value, bit32.rshift(value, 14));
	}

	public nextXorshift(): number {
		let value = this.nextUint32();
		value = bit32.bxor(value, bit32.lshift(value, 13));
		value = bit32.bxor(value, bit32.rshift(value, 17));
		return bit32.bxor(value, bit32.lshift(value, 5));
	}

	/** Uniform float in [0, 1). */
	public nextFloat(): number {
		return this.nextUint32() / UINT32_RANGE;
	}

	/** Uniform integer in [min, max] inclusive. */
	public nextInt(min: number, max: number): number {
		if (max < min) {
			[min, max] = [max, min];
		}
		const span = max - min + 1;
		return min + math.floor(this.nextFloat() * span);
	}

	public nextRange(min: number, max: number): number {
		return min + (max - min) * this.nextFloat();
	}

	public chance(probability: number): boolean {
		return this.nextFloat() < probability;
	}

	public nextSign(): number {
		return this.chance(0.5) ? 1 : -1;
	}

	public pick<T extends defined>(items: ReadonlyArray<T>): T | undefined {
		if (items.isEmpty()) {
			return undefined;
		}
		return items[this.nextInt(0, items.size() - 1)];
	}

	public shuffle<T extends defined>(items: ReadonlyArray<T>): Array<T> {
		const copy = [...items];
		for (let index = copy.size() - 1; index > 0; index--) {
			const swapIndex = this.nextInt(0, index);
			[copy[index], copy[swapIndex]] = [copy[swapIndex], copy[index]];
		}
		return copy;
	}

	public gaussian(mean = 0, deviation = 1): number {
		const u = math.max(this.nextFloat(), 1e-9);
		const v = this.nextFloat();
		return mean + deviation * math.sqrt(-2 * math.log(u)) * math.cos(2 * math.pi * v);
	}

	public unitVector(): Vector3 {
		const theta = this.nextRange(0, 2 * math.pi);
		const z = this.nextRange(-1, 1);
		const radius = math.sqrt(1 - z * z);
		return new Vector3(radius * math.cos(theta), radius * math.sin(theta), z);
	}

	/** Derive an independent stream so subsystems do not perturb each other. */
	public fork(label: string): SeededRandom {
		return new SeededRandom(bit32.bxor(this.nextUint32(), fnv1a(label)));
	}
}
