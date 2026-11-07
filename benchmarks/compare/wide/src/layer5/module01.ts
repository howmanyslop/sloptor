import { computeL4M12, L4M12Kind, BaseL4M12 } from "../layer4/module12";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M01Kind {
	Idle,
	Active,
	Done,
	Failed,
	Paused,
}

export const L5M01_FACTOR = 12;

export class BaseL5M01 {
	protected total = 77;
	public kind = L5M01Kind.Idle;

	public warp(value: number): number {
		this.total += value * L5M01_FACTOR;
		return this.total;
	}

	public advance(): L5M01Kind {
		const order = [L5M01Kind.Idle, L5M01Kind.Active, L5M01Kind.Done, L5M01Kind.Failed, L5M01Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M01(input: number): number {
	let result = input * 12 + 77;
	result += computeL4M12(input % 10);
	result += computeL0M00(input % 10);
	return result % 47742;
}

export function describeL5M01(kind: L5M01Kind): string {
	switch (kind) {
		case L5M01Kind.Idle:
			return "idle";
		case L5M01Kind.Active:
			return "active";
		case L5M01Kind.Done:
			return "done";
		case L5M01Kind.Failed:
			return "failed";
		case L5M01Kind.Paused:
			return "paused";
	}
}

export function linkL5M01(): Array<number> {
	const results = new Array<number>();
	const instanceL4M12 = new BaseL4M12();
	instanceL4M12.advance();
	results.push(instanceL4M12.read() + L4M12Kind.Locked);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
