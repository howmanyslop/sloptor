import { computeL7M10, L7M10Kind, BaseL7M10 } from "../layer7/module10";
import { computeL6M00, L6M00Kind, BaseL6M00 } from "../layer6/module00";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L8M06Kind {
	Done,
	Idle,
	Queued,
	Paused,
	Failed,
}

export const L8M06_FACTOR = 8;

export class BaseL8M06 {
	protected total = 80;
	public kind = L8M06Kind.Done;

	public tally(value: number): number {
		this.total += value * L8M06_FACTOR;
		return this.total;
	}

	public advance(): L8M06Kind {
		const order = [L8M06Kind.Done, L8M06Kind.Idle, L8M06Kind.Queued, L8M06Kind.Paused, L8M06Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M06(input: number): number {
	let result = input * 8 + 80;
	result += computeL7M10(input % 6);
	result += computeL6M00(input % 5);
	result += computeL0M03(input % 7);
	return result % 2032;
}

export function describeL8M06(kind: L8M06Kind): string {
	switch (kind) {
		case L8M06Kind.Done:
			return "done";
		case L8M06Kind.Idle:
			return "idle";
		case L8M06Kind.Queued:
			return "queued";
		case L8M06Kind.Paused:
			return "paused";
		case L8M06Kind.Failed:
			return "failed";
	}
}

export function linkL8M06(): Array<number> {
	const results = new Array<number>();
	const instanceL7M10 = new BaseL7M10();
	instanceL7M10.advance();
	results.push(instanceL7M10.read() + L7M10Kind.Locked);
	const instanceL6M00 = new BaseL6M00();
	instanceL6M00.advance();
	results.push(instanceL6M00.read() + L6M00Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
