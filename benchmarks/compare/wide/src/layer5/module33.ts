import { computeL4M36, L4M36Kind, BaseL4M36 } from "../layer4/module36";

export enum L5M33Kind {
	Idle,
	Locked,
	Active,
	Paused,
}

export const L5M33_FACTOR = 19;

export class BaseL5M33 {
	protected total = 91;
	public kind = L5M33Kind.Idle;

	public warp(value: number): number {
		this.total += value * L5M33_FACTOR;
		return this.total;
	}

	public advance(): L5M33Kind {
		const order = [L5M33Kind.Idle, L5M33Kind.Locked, L5M33Kind.Active, L5M33Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M33(input: number): number {
	let result = input * 19 + 91;
	result += computeL4M36(input % 7);
	return result % 32597;
}

export function describeL5M33(kind: L5M33Kind): string {
	switch (kind) {
		case L5M33Kind.Idle:
			return "idle";
		case L5M33Kind.Locked:
			return "locked";
		case L5M33Kind.Active:
			return "active";
		case L5M33Kind.Paused:
			return "paused";
	}
}

export function linkL5M33(): Array<number> {
	const results = new Array<number>();
	const instanceL4M36 = new BaseL4M36();
	instanceL4M36.advance();
	results.push(instanceL4M36.read() + L4M36Kind.Idle);
	return results;
}
