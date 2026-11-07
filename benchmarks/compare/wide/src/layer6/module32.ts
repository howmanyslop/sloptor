import { computeL5M30, L5M30Kind, BaseL5M30 } from "../layer5/module30";

export enum L6M32Kind {
	Idle,
	Queued,
	Active,
	Done,
	Locked,
}

export const L6M32_FACTOR = 4;

export class BaseL6M32 {
	protected total = 25;
	public kind = L6M32Kind.Idle;

	public scale(value: number): number {
		this.total += value * L6M32_FACTOR;
		return this.total;
	}

	public advance(): L6M32Kind {
		const order = [L6M32Kind.Idle, L6M32Kind.Queued, L6M32Kind.Active, L6M32Kind.Done, L6M32Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M32(input: number): number {
	let result = input * 4 + 25;
	result += computeL5M30(input % 9);
	return result % 82394;
}

export function describeL6M32(kind: L6M32Kind): string {
	switch (kind) {
		case L6M32Kind.Idle:
			return "idle";
		case L6M32Kind.Queued:
			return "queued";
		case L6M32Kind.Active:
			return "active";
		case L6M32Kind.Done:
			return "done";
		case L6M32Kind.Locked:
			return "locked";
	}
}

export function linkL6M32(): Array<number> {
	const results = new Array<number>();
	const instanceL5M30 = new BaseL5M30();
	instanceL5M30.advance();
	results.push(instanceL5M30.read() + L5M30Kind.Idle);
	return results;
}
