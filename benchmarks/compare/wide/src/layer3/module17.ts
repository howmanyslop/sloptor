import { computeL2M02, L2M02Kind, BaseL2M02 } from "../layer2/module02";
import { computeL0M11, L0M11Kind, BaseL0M11 } from "../layer0/module11";
import { computeL2M23, L2M23Kind, BaseL2M23 } from "../layer2/module23";

export enum L3M17Kind {
	Done,
	Failed,
	Queued,
}

export const L3M17_FACTOR = 10;

export class BaseL3M17 {
	protected total = 90;
	public kind = L3M17Kind.Done;

	public scale(value: number): number {
		this.total += value * L3M17_FACTOR;
		return this.total;
	}

	public advance(): L3M17Kind {
		const order = [L3M17Kind.Done, L3M17Kind.Failed, L3M17Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL3M17 extends BaseL2M02 {
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

export function computeL3M17(input: number): number {
	let result = input * 10 + 90;
	result += computeL2M02(input % 6);
	result += computeL0M11(input % 7);
	result += computeL2M23(input % 5);
	return result % 87928;
}

export function describeL3M17(kind: L3M17Kind): string {
	switch (kind) {
		case L3M17Kind.Done:
			return "done";
		case L3M17Kind.Failed:
			return "failed";
		case L3M17Kind.Queued:
			return "queued";
	}
}

export function linkL3M17(): Array<number> {
	const results = new Array<number>();
	const instanceL2M02 = new BaseL2M02();
	instanceL2M02.advance();
	results.push(instanceL2M02.read() + L2M02Kind.Active);
	const instanceL0M11 = new BaseL0M11();
	instanceL0M11.advance();
	results.push(instanceL0M11.read() + L0M11Kind.Active);
	const instanceL2M23 = new BaseL2M23();
	instanceL2M23.advance();
	results.push(instanceL2M23.read() + L2M23Kind.Queued);
	return results;
}
