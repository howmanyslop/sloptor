export const EPSILON = 1e-6;

export function clamp(value: number, min: number, max: number): number {
	return math.max(min, math.min(max, value));
}

export function lerp(a: number, b: number, alpha: number): number {
	return a + (b - a) * alpha;
}

export function inverseLerp(a: number, b: number, value: number): number {
	if (math.abs(b - a) < EPSILON) {
		return 0;
	}
	return (value - a) / (b - a);
}

export function remap(value: number, inMin: number, inMax: number, outMin: number, outMax: number): number {
	return lerp(outMin, outMax, clamp(inverseLerp(inMin, inMax, value), 0, 1));
}

export function smoothstep(edge0: number, edge1: number, value: number): number {
	const t = clamp(inverseLerp(edge0, edge1, value), 0, 1);
	return t * t * (3 - 2 * t);
}

export function roundTo(value: number, step: number): number {
	return math.floor(value / step + 0.5) * step;
}

export function approximately(a: number, b: number, tolerance = EPSILON): boolean {
	return math.abs(a - b) <= tolerance;
}

export function horizontal(vector: Vector3): Vector3 {
	return new Vector3(vector.X, 0, vector.Z);
}

export function horizontalDistance(a: Vector3, b: Vector3): number {
	return horizontal(b.sub(a)).Magnitude;
}

export function pingPong(time: number, length: number): number {
	const cycle = time % (length * 2);
	return cycle <= length ? cycle : length * 2 - cycle;
}

export function sum(values: ReadonlyArray<number>): number {
	return values.reduce((total, value) => total + value, 0);
}

export function mean(values: ReadonlyArray<number>): number {
	return values.isEmpty() ? 0 : sum(values) / values.size();
}

export function percentile(values: ReadonlyArray<number>, fraction: number): number {
	if (values.isEmpty()) {
		return 0;
	}
	const sorted = [...values].sort((a, b) => a < b);
	const index = clamp(math.floor(fraction * (sorted.size() - 1) + 0.5), 0, sorted.size() - 1);
	return sorted[index];
}

export function vectorMin(a: Vector3, b: Vector3): Vector3 {
	return new Vector3(math.min(a.X, b.X), math.min(a.Y, b.Y), math.min(a.Z, b.Z));
}

export function vectorMax(a: Vector3, b: Vector3): Vector3 {
	return new Vector3(math.max(a.X, b.X), math.max(a.Y, b.Y), math.max(a.Z, b.Z));
}

export function rotateY(vector: Vector3, radians: number): Vector3 {
	const cosine = math.cos(radians);
	const sine = math.sin(radians);
	return new Vector3(vector.X * cosine - vector.Z * sine, vector.Y, vector.X * sine + vector.Z * cosine);
}
