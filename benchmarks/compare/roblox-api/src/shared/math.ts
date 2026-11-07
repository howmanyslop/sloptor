export function lerp(from: number, to: number, alpha: number): number {
	return from + (to - from) * alpha;
}

export function clamp01(value: number): number {
	return math.clamp(value, 0, 1);
}

export function remap(value: number, inMin: number, inMax: number, outMin: number, outMax: number): number {
	return outMin + ((value - inMin) / (inMax - inMin)) * (outMax - outMin);
}

export function flatten(vector: Vector3): Vector3 {
	return new Vector3(vector.X, 0, vector.Z);
}

export function horizontalDistance(first: Vector3, second: Vector3): number {
	return flatten(first.sub(second)).Magnitude;
}

export function randomPointInBox(random: Random, center: CFrame, size: Vector3): Vector3 {
	const offset = new Vector3(
		random.NextNumber(-size.X / 2, size.X / 2),
		random.NextNumber(-size.Y / 2, size.Y / 2),
		random.NextNumber(-size.Z / 2, size.Z / 2),
	);
	return center.PointToWorldSpace(offset);
}

export function pointsOnCircle(center: Vector3, radius: number, count: number): Array<Vector3> {
	const points = new Array<Vector3>();
	for (let index = 0; index < count; index++) {
		const angle = (index / count) * math.pi * 2;
		points.push(center.add(new Vector3(math.cos(angle) * radius, 0, math.sin(angle) * radius)));
	}
	return points;
}

export function lookAtFlat(origin: Vector3, target: Vector3): CFrame {
	const flatTarget = new Vector3(target.X, origin.Y, target.Z);
	return CFrame.lookAt(origin, flatTarget);
}

export function bezier(p0: Vector3, p1: Vector3, p2: Vector3, alpha: number): Vector3 {
	const a = p0.Lerp(p1, alpha);
	const b = p1.Lerp(p2, alpha);
	return a.Lerp(b, alpha);
}

export function orbit(center: CFrame, radius: number, angle: number, height: number): CFrame {
	return center.mul(CFrame.Angles(0, angle, 0)).mul(new CFrame(0, height, radius));
}

export function colorRamp(stops: ReadonlyArray<Color3>, alpha: number): Color3 {
	if (stops.size() === 1) return stops[0];
	const scaled = clamp01(alpha) * (stops.size() - 1);
	const index = math.min(math.floor(scaled), stops.size() - 2);
	return stops[index].Lerp(stops[index + 1], scaled - index);
}

export function hueShift(color: Color3, amount: number): Color3 {
	const [hue, saturation, value] = color.ToHSV();
	return Color3.fromHSV((hue + amount) % 1, saturation, value);
}

export function springStep(position: number, velocity: number, target: number, stiffness: number, damping: number, dt: number): [number, number] {
	const force = (target - position) * stiffness - velocity * damping;
	const newVelocity = velocity + force * dt;
	return [position + newVelocity * dt, newVelocity];
}

export function formatTime(seconds: number): string {
	const whole = math.max(0, math.floor(seconds));
	return string.format("%d:%02d", math.floor(whole / 60), whole % 60);
}

export function formatNumber(value: number): string {
	const suffixes = ["", "K", "M", "B"];
	let scaled = value;
	let index = 0;
	while (math.abs(scaled) >= 1000 && index < suffixes.size() - 1) {
		scaled /= 1000;
		index++;
	}
	return index === 0 ? tostring(value) : string.format("%.1f%s", scaled, suffixes[index]);
}
