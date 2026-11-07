import { computeL6M10, L6M10Kind, BaseL6M10 } from "../layer6/module10";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L7M29Kind {
	Active,
	Done,
	Paused,
	Failed,
	Idle,
}

export const L7M29_FACTOR = 8;

export class BaseL7M29 {
	protected total = 25;
	public kind = L7M29Kind.Active;

	public scale(value: number): number {
		this.total += value * L7M29_FACTOR;
		return this.total;
	}

	public advance(): L7M29Kind {
		const order = [L7M29Kind.Active, L7M29Kind.Done, L7M29Kind.Paused, L7M29Kind.Failed, L7M29Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M29(input: number): number {
	let result = input * 8 + 25;
	result += computeL6M10(input % 10);
	result += computeL0M03(input % 5);
	return result % 22048;
}

export function describeL7M29(kind: L7M29Kind): string {
	switch (kind) {
		case L7M29Kind.Active:
			return "active";
		case L7M29Kind.Done:
			return "done";
		case L7M29Kind.Paused:
			return "paused";
		case L7M29Kind.Failed:
			return "failed";
		case L7M29Kind.Idle:
			return "idle";
	}
}

export function linkL7M29(): Array<number> {
	const results = new Array<number>();
	const instanceL6M10 = new BaseL6M10();
	instanceL6M10.advance();
	results.push(instanceL6M10.read() + L6M10Kind.Locked);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
