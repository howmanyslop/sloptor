import { computeL7M31, L7M31Kind, BaseL7M31 } from "../layer7/module31";
import { computeL3M26, L3M26Kind, BaseL3M26 } from "../layer3/module26";
import { computeL7M24, L7M24Kind, BaseL7M24 } from "../layer7/module24";

export enum L8M09Kind {
	Idle,
	Active,
	Paused,
	Done,
}

export const L8M09_FACTOR = 18;

export class BaseL8M09 {
	protected total = 40;
	public kind = L8M09Kind.Idle;

	public tally(value: number): number {
		this.total += value * L8M09_FACTOR;
		return this.total;
	}

	public advance(): L8M09Kind {
		const order = [L8M09Kind.Idle, L8M09Kind.Active, L8M09Kind.Paused, L8M09Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M09(input: number): number {
	let result = input * 18 + 40;
	result += computeL7M31(input % 3);
	result += computeL3M26(input % 10);
	result += computeL7M24(input % 11);
	return result % 98587;
}

export function describeL8M09(kind: L8M09Kind): string {
	switch (kind) {
		case L8M09Kind.Idle:
			return "idle";
		case L8M09Kind.Active:
			return "active";
		case L8M09Kind.Paused:
			return "paused";
		case L8M09Kind.Done:
			return "done";
	}
}

export function linkL8M09(): Array<number> {
	const results = new Array<number>();
	const instanceL7M31 = new BaseL7M31();
	instanceL7M31.advance();
	results.push(instanceL7M31.read() + L7M31Kind.Queued);
	const instanceL3M26 = new BaseL3M26();
	instanceL3M26.advance();
	results.push(instanceL3M26.read() + L3M26Kind.Failed);
	const instanceL7M24 = new BaseL7M24();
	instanceL7M24.advance();
	results.push(instanceL7M24.read() + L7M24Kind.Idle);
	return results;
}
