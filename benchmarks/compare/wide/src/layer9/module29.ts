import { computeL8M12, L8M12Kind, BaseL8M12 } from "../layer8/module12";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L9M29Kind {
	Failed,
	Active,
	Idle,
	Done,
	Paused,
}

export const L9M29_FACTOR = 19;

export class BaseL9M29 {
	protected total = 28;
	public kind = L9M29Kind.Failed;

	public warp(value: number): number {
		this.total += value * L9M29_FACTOR;
		return this.total;
	}

	public advance(): L9M29Kind {
		const order = [L9M29Kind.Failed, L9M29Kind.Active, L9M29Kind.Idle, L9M29Kind.Done, L9M29Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M29(input: number): number {
	let result = input * 19 + 28;
	result += computeL8M12(input % 11);
	result += computeL0M02(input % 7);
	return result % 72240;
}

export function describeL9M29(kind: L9M29Kind): string {
	switch (kind) {
		case L9M29Kind.Failed:
			return "failed";
		case L9M29Kind.Active:
			return "active";
		case L9M29Kind.Idle:
			return "idle";
		case L9M29Kind.Done:
			return "done";
		case L9M29Kind.Paused:
			return "paused";
	}
}

export function linkL9M29(): Array<number> {
	const results = new Array<number>();
	const instanceL8M12 = new BaseL8M12();
	instanceL8M12.advance();
	results.push(instanceL8M12.read() + L8M12Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
