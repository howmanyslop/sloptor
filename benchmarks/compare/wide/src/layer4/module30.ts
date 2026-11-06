import { computeL3M14, L3M14Kind, BaseL3M14 } from "../layer3/module14";

export enum L4M30Kind {
	Idle,
	Done,
	Active,
	Locked,
}

export const L4M30_FACTOR = 12;

export class BaseL4M30 {
	protected total = 14;
	public kind = L4M30Kind.Idle;

	public scale(value: number): number {
		this.total += value * L4M30_FACTOR;
		return this.total;
	}

	public advance(): L4M30Kind {
		const order = [L4M30Kind.Idle, L4M30Kind.Done, L4M30Kind.Active, L4M30Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M30(input: number): number {
	let result = input * 12 + 14;
	result += computeL3M14(input % 3);
	return result % 50527;
}

export function describeL4M30(kind: L4M30Kind): string {
	switch (kind) {
		case L4M30Kind.Idle:
			return "idle";
		case L4M30Kind.Done:
			return "done";
		case L4M30Kind.Active:
			return "active";
		case L4M30Kind.Locked:
			return "locked";
	}
}

export function linkL4M30(): Array<number> {
	const results = new Array<number>();
	const instanceL3M14 = new BaseL3M14();
	instanceL3M14.advance();
	results.push(instanceL3M14.read() + L3M14Kind.Idle);
	return results;
}
