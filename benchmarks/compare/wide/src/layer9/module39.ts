import { computeL8M02, L8M02Kind, BaseL8M02 } from "../layer8/module02";
import { computeL7M35, L7M35Kind, BaseL7M35 } from "../layer7/module35";

export enum L9M39Kind {
	Paused,
	Done,
	Locked,
	Failed,
	Queued,
}

export const L9M39_FACTOR = 8;

export class BaseL9M39 {
	protected total = 21;
	public kind = L9M39Kind.Paused;

	public scale(value: number): number {
		this.total += value * L9M39_FACTOR;
		return this.total;
	}

	public advance(): L9M39Kind {
		const order = [L9M39Kind.Paused, L9M39Kind.Done, L9M39Kind.Locked, L9M39Kind.Failed, L9M39Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL9M39 extends BaseL8M02 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M39(input: number): number {
	let result = input * 8 + 21;
	result += computeL8M02(input % 9);
	result += computeL7M35(input % 8);
	return result % 18874;
}

export function describeL9M39(kind: L9M39Kind): string {
	switch (kind) {
		case L9M39Kind.Paused:
			return "paused";
		case L9M39Kind.Done:
			return "done";
		case L9M39Kind.Locked:
			return "locked";
		case L9M39Kind.Failed:
			return "failed";
		case L9M39Kind.Queued:
			return "queued";
	}
}

export function linkL9M39(): Array<number> {
	const results = new Array<number>();
	const instanceL8M02 = new BaseL8M02();
	instanceL8M02.advance();
	results.push(instanceL8M02.read() + L8M02Kind.Paused);
	const instanceL7M35 = new BaseL7M35();
	instanceL7M35.advance();
	results.push(instanceL7M35.read() + L7M35Kind.Locked);
	return results;
}
