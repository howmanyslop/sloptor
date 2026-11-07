import { computeL6M24, L6M24Kind, BaseL6M24 } from "../layer6/module24";

export enum L7M31Kind {
	Queued,
	Idle,
	Active,
	Locked,
}

export const L7M31_FACTOR = 15;

export class BaseL7M31 {
	protected total = 8;
	public kind = L7M31Kind.Queued;

	public shift(value: number): number {
		this.total += value * L7M31_FACTOR;
		return this.total;
	}

	public advance(): L7M31Kind {
		const order = [L7M31Kind.Queued, L7M31Kind.Idle, L7M31Kind.Active, L7M31Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M31(input: number): number {
	let result = input * 15 + 8;
	result += computeL6M24(input % 10);
	return result % 11272;
}

export function describeL7M31(kind: L7M31Kind): string {
	switch (kind) {
		case L7M31Kind.Queued:
			return "queued";
		case L7M31Kind.Idle:
			return "idle";
		case L7M31Kind.Active:
			return "active";
		case L7M31Kind.Locked:
			return "locked";
	}
}

export function linkL7M31(): Array<number> {
	const results = new Array<number>();
	const instanceL6M24 = new BaseL6M24();
	instanceL6M24.advance();
	results.push(instanceL6M24.read() + L6M24Kind.Queued);
	return results;
}
