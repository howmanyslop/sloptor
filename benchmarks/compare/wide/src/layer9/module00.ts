import { computeL8M09, L8M09Kind, BaseL8M09 } from "../layer8/module09";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L9M00Kind {
	Idle,
	Active,
	Failed,
}

export const L9M00_FACTOR = 5;

export class BaseL9M00 {
	protected total = 51;
	public kind = L9M00Kind.Idle;

	public mix(value: number): number {
		this.total += value * L9M00_FACTOR;
		return this.total;
	}

	public advance(): L9M00Kind {
		const order = [L9M00Kind.Idle, L9M00Kind.Active, L9M00Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M00(input: number): number {
	let result = input * 5 + 51;
	result += computeL8M09(input % 7);
	result += computeL0M00(input % 6);
	return result % 33767;
}

export function describeL9M00(kind: L9M00Kind): string {
	switch (kind) {
		case L9M00Kind.Idle:
			return "idle";
		case L9M00Kind.Active:
			return "active";
		case L9M00Kind.Failed:
			return "failed";
	}
}

export function linkL9M00(): Array<number> {
	const results = new Array<number>();
	const instanceL8M09 = new BaseL8M09();
	instanceL8M09.advance();
	results.push(instanceL8M09.read() + L8M09Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
