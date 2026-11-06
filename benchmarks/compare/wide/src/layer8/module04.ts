import { computeL7M36, L7M36Kind, BaseL7M36 } from "../layer7/module36";
import { computeL3M22, L3M22Kind, BaseL3M22 } from "../layer3/module22";
import { computeL7M01, L7M01Kind, BaseL7M01 } from "../layer7/module01";

export enum L8M04Kind {
	Failed,
	Locked,
	Queued,
}

export const L8M04_FACTOR = 10;

export class BaseL8M04 {
	protected total = 17;
	public kind = L8M04Kind.Failed;

	public tally(value: number): number {
		this.total += value * L8M04_FACTOR;
		return this.total;
	}

	public advance(): L8M04Kind {
		const order = [L8M04Kind.Failed, L8M04Kind.Locked, L8M04Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL8M04 extends BaseL7M36 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M04(input: number): number {
	let result = input * 10 + 17;
	result += computeL7M36(input % 6);
	result += computeL3M22(input % 11);
	result += computeL7M01(input % 3);
	return result % 68725;
}

export function describeL8M04(kind: L8M04Kind): string {
	switch (kind) {
		case L8M04Kind.Failed:
			return "failed";
		case L8M04Kind.Locked:
			return "locked";
		case L8M04Kind.Queued:
			return "queued";
	}
}

export function linkL8M04(): Array<number> {
	const results = new Array<number>();
	const instanceL7M36 = new BaseL7M36();
	instanceL7M36.advance();
	results.push(instanceL7M36.read() + L7M36Kind.Failed);
	const instanceL3M22 = new BaseL3M22();
	instanceL3M22.advance();
	results.push(instanceL3M22.read() + L3M22Kind.Paused);
	const instanceL7M01 = new BaseL7M01();
	instanceL7M01.advance();
	results.push(instanceL7M01.read() + L7M01Kind.Failed);
	return results;
}
