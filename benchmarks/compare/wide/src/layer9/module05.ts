import { computeL8M04, L8M04Kind, BaseL8M04 } from "../layer8/module04";
import { computeL7M04, L7M04Kind, BaseL7M04 } from "../layer7/module04";

export enum L9M05Kind {
	Paused,
	Locked,
	Idle,
	Queued,
}

export const L9M05_FACTOR = 2;

export class BaseL9M05 {
	protected total = 87;
	public kind = L9M05Kind.Paused;

	public tally(value: number): number {
		this.total += value * L9M05_FACTOR;
		return this.total;
	}

	public advance(): L9M05Kind {
		const order = [L9M05Kind.Paused, L9M05Kind.Locked, L9M05Kind.Idle, L9M05Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL9M05 extends BaseL9M05 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M05(input: number): number {
	let result = input * 2 + 87;
	result += computeL8M04(input % 6);
	result += computeL7M04(input % 4);
	return result % 6357;
}

export function describeL9M05(kind: L9M05Kind): string {
	switch (kind) {
		case L9M05Kind.Paused:
			return "paused";
		case L9M05Kind.Locked:
			return "locked";
		case L9M05Kind.Idle:
			return "idle";
		case L9M05Kind.Queued:
			return "queued";
	}
}

export function linkL9M05(): Array<number> {
	const results = new Array<number>();
	const instanceL8M04 = new BaseL8M04();
	instanceL8M04.advance();
	results.push(instanceL8M04.read() + L8M04Kind.Failed);
	const instanceL7M04 = new BaseL7M04();
	instanceL7M04.advance();
	results.push(instanceL7M04.read() + L7M04Kind.Done);
	return results;
}
