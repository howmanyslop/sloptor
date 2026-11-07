import { PHYSICS } from "../config";
import type { PhysicsConfig } from "../config";
import { horizontal } from "../util/math";

export interface JumpProfile {
	readonly apexHeight: number;
	readonly airTimeFlat: number;
	readonly maxFlatDistance: number;
}

export type JumpVerdict =
	| { readonly status: "trivial"; readonly margin: number }
	| { readonly status: "jump"; readonly margin: number; readonly airTime: number }
	| { readonly status: "tooHigh"; readonly deficit: number }
	| { readonly status: "tooFar"; readonly deficit: number }
	| { readonly status: "blocked"; readonly reason: string };

export function jumpProfile(physics: PhysicsConfig = PHYSICS): JumpProfile {
	const { gravity, jumpPower, walkSpeed } = physics;
	const apexHeight = (jumpPower * jumpPower) / (2 * gravity);
	const airTimeFlat = (2 * jumpPower) / gravity;
	return { apexHeight, airTimeFlat, maxFlatDistance: airTimeFlat * walkSpeed };
}

/** Time until a jump returns to `heightDelta` relative to takeoff, on the way down. */
export function airTimeTo(heightDelta: number, physics: PhysicsConfig = PHYSICS): number | undefined {
	const { gravity, jumpPower } = physics;
	const discriminant = jumpPower * jumpPower - 2 * gravity * heightDelta;
	if (discriminant < 0) {
		return undefined;
	}
	return (jumpPower + math.sqrt(discriminant)) / gravity;
}

/**
 * Decide whether a player standing on `from` can land on `to`. Both points are
 * top-surface centres; `landingRadius` lets a wide platform absorb some error.
 */
export function evaluateJump(
	from: Vector3,
	to: Vector3,
	landingRadius = 2,
	physics: PhysicsConfig = PHYSICS,
): JumpVerdict {
	const offset = to.sub(from);
	const rise = offset.Y;
	const distance = math.max(0, horizontal(offset).Magnitude - landingRadius);
	const profile = jumpProfile(physics);
	const usableApex = profile.apexHeight * physics.safetyMargin;

	if (rise > usableApex) {
		return { status: "tooHigh", deficit: rise - usableApex };
	}
	if (distance <= 1 && math.abs(rise) <= 1) {
		return { status: "trivial", margin: 1 };
	}
	const airTime = airTimeTo(rise, physics);
	if (airTime === undefined) {
		return { status: "blocked", reason: "no real trajectory" };
	}
	const reach = airTime * physics.walkSpeed * physics.safetyMargin;
	if (distance > reach) {
		return { status: "tooFar", deficit: distance - reach };
	}
	return { status: "jump", margin: 1 - distance / reach, airTime };
}

export function isReachable(verdict: JumpVerdict): boolean {
	return verdict.status === "trivial" || verdict.status === "jump";
}

export function jumpCost(verdict: JumpVerdict): number {
	switch (verdict.status) {
		case "trivial":
			return 1;
		case "jump":
			return 1 + (1 - verdict.margin) * 3;
		case "tooHigh":
		case "tooFar":
		case "blocked":
			return math.huge;
	}
}

/** Sample the parabola for debug visualisation. */
export function trajectoryPoints(from: Vector3, to: Vector3, samples = 12, physics: PhysicsConfig = PHYSICS): Array<Vector3> {
	const airTime = airTimeTo(to.Y - from.Y, physics) ?? 1;
	const flat = horizontal(to.sub(from));
	const points = new Array<Vector3>();
	for (let index = 0; index <= samples; index++) {
		const t = (index / samples) * airTime;
		const y = physics.jumpPower * t - 0.5 * physics.gravity * t * t;
		points.push(from.add(flat.mul(t / airTime)).add(new Vector3(0, y, 0)));
	}
	return points;
}
