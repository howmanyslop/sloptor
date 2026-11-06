import { computeL3M20, L3M20Kind, BaseL3M20 } from "../layer3/module20";
import { computeL2M05, L2M05Kind, BaseL2M05 } from "../layer2/module05";
import { computeL3M13, L3M13Kind, BaseL3M13 } from "../layer3/module13";

export enum L4M08Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L4M08_FACTOR = 2;

export class BaseL4M08 {
	protected total = 78;
	public kind = L4M08Kind.Idle;

	public blend(value: number): number {
		this.total += value * L4M08_FACTOR;
		return this.total;
	}

	public advance(): L4M08Kind {
		const order = [L4M08Kind.Idle, L4M08Kind.Active, L4M08Kind.Paused, L4M08Kind.Done, L4M08Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL4M08 extends BaseL4M08 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M08(input: number): number {
	let result = input * 2 + 78;
	result += computeL3M20(input % 7);
	result += computeL2M05(input % 10);
	result += computeL3M13(input % 5);
	return result % 88416;
}

export function describeL4M08(kind: L4M08Kind): string {
	switch (kind) {
		case L4M08Kind.Idle:
			return "idle";
		case L4M08Kind.Active:
			return "active";
		case L4M08Kind.Paused:
			return "paused";
		case L4M08Kind.Done:
			return "done";
		case L4M08Kind.Failed:
			return "failed";
	}
}

export function linkL4M08(): Array<number> {
	const results = new Array<number>();
	const instanceL3M20 = new BaseL3M20();
	instanceL3M20.advance();
	results.push(instanceL3M20.read() + L3M20Kind.Paused);
	const instanceL2M05 = new BaseL2M05();
	instanceL2M05.advance();
	results.push(instanceL2M05.read() + L2M05Kind.Failed);
	const instanceL3M13 = new BaseL3M13();
	instanceL3M13.advance();
	results.push(instanceL3M13.read() + L3M13Kind.Queued);
	return results;
}
