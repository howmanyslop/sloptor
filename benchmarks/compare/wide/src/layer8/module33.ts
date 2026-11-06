import { computeL7M39, L7M39Kind, BaseL7M39 } from "../layer7/module39";

export enum L8M33Kind {
	Done,
	Paused,
	Queued,
	Failed,
	Locked,
}

export const L8M33_FACTOR = 5;

export class BaseL8M33 {
	protected total = 87;
	public kind = L8M33Kind.Done;

	public tally(value: number): number {
		this.total += value * L8M33_FACTOR;
		return this.total;
	}

	public advance(): L8M33Kind {
		const order = [L8M33Kind.Done, L8M33Kind.Paused, L8M33Kind.Queued, L8M33Kind.Failed, L8M33Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M33(input: number): number {
	let result = input * 5 + 87;
	result += computeL7M39(input % 10);
	return result % 25557;
}

export function describeL8M33(kind: L8M33Kind): string {
	switch (kind) {
		case L8M33Kind.Done:
			return "done";
		case L8M33Kind.Paused:
			return "paused";
		case L8M33Kind.Queued:
			return "queued";
		case L8M33Kind.Failed:
			return "failed";
		case L8M33Kind.Locked:
			return "locked";
	}
}

export function linkL8M33(): Array<number> {
	const results = new Array<number>();
	const instanceL7M39 = new BaseL7M39();
	instanceL7M39.advance();
	results.push(instanceL7M39.read() + L7M39Kind.Queued);
	return results;
}
