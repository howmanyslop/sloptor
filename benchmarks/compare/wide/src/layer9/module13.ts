import { computeL8M20, L8M20Kind, BaseL8M20 } from "../layer8/module20";
import { computeL3M31, L3M31Kind, BaseL3M31 } from "../layer3/module31";

export enum L9M13Kind {
	Failed,
	Done,
	Paused,
	Idle,
	Active,
}

export const L9M13_FACTOR = 5;

export class BaseL9M13 {
	protected total = 88;
	public kind = L9M13Kind.Failed;

	public tally(value: number): number {
		this.total += value * L9M13_FACTOR;
		return this.total;
	}

	public advance(): L9M13Kind {
		const order = [L9M13Kind.Failed, L9M13Kind.Done, L9M13Kind.Paused, L9M13Kind.Idle, L9M13Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M13(input: number): number {
	let result = input * 5 + 88;
	result += computeL8M20(input % 11);
	result += computeL3M31(input % 6);
	return result % 13548;
}

export function describeL9M13(kind: L9M13Kind): string {
	switch (kind) {
		case L9M13Kind.Failed:
			return "failed";
		case L9M13Kind.Done:
			return "done";
		case L9M13Kind.Paused:
			return "paused";
		case L9M13Kind.Idle:
			return "idle";
		case L9M13Kind.Active:
			return "active";
	}
}

export function linkL9M13(): Array<number> {
	const results = new Array<number>();
	const instanceL8M20 = new BaseL8M20();
	instanceL8M20.advance();
	results.push(instanceL8M20.read() + L8M20Kind.Done);
	const instanceL3M31 = new BaseL3M31();
	instanceL3M31.advance();
	results.push(instanceL3M31.read() + L3M31Kind.Idle);
	return results;
}
