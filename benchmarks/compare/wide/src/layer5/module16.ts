import { computeL4M37, L4M37Kind, BaseL4M37 } from "../layer4/module37";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L5M16Kind {
	Done,
	Locked,
	Paused,
	Active,
}

export const L5M16_FACTOR = 19;

export class BaseL5M16 {
	protected total = 24;
	public kind = L5M16Kind.Done;

	public warp(value: number): number {
		this.total += value * L5M16_FACTOR;
		return this.total;
	}

	public advance(): L5M16Kind {
		const order = [L5M16Kind.Done, L5M16Kind.Locked, L5M16Kind.Paused, L5M16Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M16(input: number): number {
	let result = input * 19 + 24;
	result += computeL4M37(input % 3);
	result += computeL0M02(input % 11);
	return result % 53944;
}

export function describeL5M16(kind: L5M16Kind): string {
	switch (kind) {
		case L5M16Kind.Done:
			return "done";
		case L5M16Kind.Locked:
			return "locked";
		case L5M16Kind.Paused:
			return "paused";
		case L5M16Kind.Active:
			return "active";
	}
}

export function linkL5M16(): Array<number> {
	const results = new Array<number>();
	const instanceL4M37 = new BaseL4M37();
	instanceL4M37.advance();
	results.push(instanceL4M37.read() + L4M37Kind.Queued);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
