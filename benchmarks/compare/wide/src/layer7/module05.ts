import { computeL6M02, L6M02Kind, BaseL6M02 } from "../layer6/module02";

export enum L7M05Kind {
	Failed,
	Locked,
	Paused,
	Done,
	Idle,
}

export const L7M05_FACTOR = 10;

export class BaseL7M05 {
	protected total = 33;
	public kind = L7M05Kind.Failed;

	public shift(value: number): number {
		this.total += value * L7M05_FACTOR;
		return this.total;
	}

	public advance(): L7M05Kind {
		const order = [L7M05Kind.Failed, L7M05Kind.Locked, L7M05Kind.Paused, L7M05Kind.Done, L7M05Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M05(input: number): number {
	let result = input * 10 + 33;
	result += computeL6M02(input % 11);
	return result % 46814;
}

export function describeL7M05(kind: L7M05Kind): string {
	switch (kind) {
		case L7M05Kind.Failed:
			return "failed";
		case L7M05Kind.Locked:
			return "locked";
		case L7M05Kind.Paused:
			return "paused";
		case L7M05Kind.Done:
			return "done";
		case L7M05Kind.Idle:
			return "idle";
	}
}

export function linkL7M05(): Array<number> {
	const results = new Array<number>();
	const instanceL6M02 = new BaseL6M02();
	instanceL6M02.advance();
	results.push(instanceL6M02.read() + L6M02Kind.Paused);
	return results;
}
