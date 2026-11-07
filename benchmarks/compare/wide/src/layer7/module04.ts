import { computeL6M35, L6M35Kind, BaseL6M35 } from "../layer6/module35";

export enum L7M04Kind {
	Done,
	Paused,
	Queued,
}

export const L7M04_FACTOR = 5;

export class BaseL7M04 {
	protected total = 12;
	public kind = L7M04Kind.Done;

	public shift(value: number): number {
		this.total += value * L7M04_FACTOR;
		return this.total;
	}

	public advance(): L7M04Kind {
		const order = [L7M04Kind.Done, L7M04Kind.Paused, L7M04Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M04(input: number): number {
	let result = input * 5 + 12;
	result += computeL6M35(input % 8);
	return result % 90498;
}

export function describeL7M04(kind: L7M04Kind): string {
	switch (kind) {
		case L7M04Kind.Done:
			return "done";
		case L7M04Kind.Paused:
			return "paused";
		case L7M04Kind.Queued:
			return "queued";
	}
}

export function linkL7M04(): Array<number> {
	const results = new Array<number>();
	const instanceL6M35 = new BaseL6M35();
	instanceL6M35.advance();
	results.push(instanceL6M35.read() + L6M35Kind.Locked);
	return results;
}
