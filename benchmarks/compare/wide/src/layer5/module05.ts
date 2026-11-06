import { computeL4M00, L4M00Kind, BaseL4M00 } from "../layer4/module00";
import { computeL4M29, L4M29Kind, BaseL4M29 } from "../layer4/module29";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M05Kind {
	Paused,
	Active,
	Done,
}

export const L5M05_FACTOR = 19;

export class BaseL5M05 {
	protected total = 29;
	public kind = L5M05Kind.Paused;

	public mix(value: number): number {
		this.total += value * L5M05_FACTOR;
		return this.total;
	}

	public advance(): L5M05Kind {
		const order = [L5M05Kind.Paused, L5M05Kind.Active, L5M05Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M05(input: number): number {
	let result = input * 19 + 29;
	result += computeL4M00(input % 8);
	result += computeL4M29(input % 9);
	result += computeL0M00(input % 10);
	return result % 98313;
}

export function describeL5M05(kind: L5M05Kind): string {
	switch (kind) {
		case L5M05Kind.Paused:
			return "paused";
		case L5M05Kind.Active:
			return "active";
		case L5M05Kind.Done:
			return "done";
	}
}

export function linkL5M05(): Array<number> {
	const results = new Array<number>();
	const instanceL4M00 = new BaseL4M00();
	instanceL4M00.advance();
	results.push(instanceL4M00.read() + L4M00Kind.Idle);
	const instanceL4M29 = new BaseL4M29();
	instanceL4M29.advance();
	results.push(instanceL4M29.read() + L4M29Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
