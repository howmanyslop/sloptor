const TWO_POW_32 = 4294967296;

export function toUint32(value: number): number {
	const wrapped = math.floor(value) % TWO_POW_32;
	return wrapped < 0 ? wrapped + TWO_POW_32 : wrapped;
}

/**
 * 32-bit integer multiply that never loses precision in a double: each operand
 * is split into 16-bit halves so no intermediate product exceeds 2^32.
 */
export function imul32(a: number, b: number): number {
	const aHigh = bit32.rshift(a, 16);
	const aLow = bit32.band(a, 0xffff);
	const bHigh = bit32.rshift(b, 16);
	const bLow = bit32.band(b, 0xffff);
	const cross = bit32.band(aHigh * bLow + aLow * bHigh, 0xffff);
	return toUint32(bit32.lshift(cross, 16) + aLow * bLow);
}

export function add32(a: number, b: number): number {
	return toUint32(a + b);
}

export function rotateLeft32(value: number, amount: number): number {
	return bit32.lrotate(value, amount);
}

export function mixBits(seed: number): number {
	let hash = toUint32(seed);
	hash = bit32.bxor(hash, bit32.rshift(hash, 16));
	hash = imul32(hash, 0x85ebca6b);
	hash = bit32.bxor(hash, bit32.rshift(hash, 13));
	hash = imul32(hash, 0xc2b2ae35);
	return bit32.bxor(hash, bit32.rshift(hash, 16));
}

export function fnv1a(text: string): number {
	let hash = 0x811c9dc5;
	for (let position = 1; position <= text.size(); position++) {
		const [byte] = string.byte(text, position);
		hash = imul32(bit32.bxor(hash, byte ?? 0), 0x01000193);
	}
	return hash;
}

export function combineHashes(...parts: Array<number>): number {
	return parts.reduce((accumulator, part) => mixBits(bit32.bxor(rotateLeft32(accumulator, 5), part)), 0x9e3779b9);
}
