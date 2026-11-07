import { computeL2M15, L2M15Kind, BaseL2M15 } from "../layer2/module15";
import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";

export enum L3M36Kind {
	Done,
	Idle,
	Failed,
}

export const L3M36_FACTOR = 9;

export class BaseL3M36 {
	protected total = 84;
	public kind = L3M36Kind.Done;

	public weigh(value: number): number {
		this.total += value * L3M36_FACTOR;
		return this.total;
	}

	public advance(): L3M36Kind {
		const order = [L3M36Kind.Done, L3M36Kind.Idle, L3M36Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL3M36 extends BaseL2M15 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M36(input: number): number {
	let result = input * 9 + 84;
	result += computeL2M15(input % 11);
	result += computeL0M28(input % 8);
	return result % 42771;
}

export function describeL3M36(kind: L3M36Kind): string {
	switch (kind) {
		case L3M36Kind.Done:
			return "done";
		case L3M36Kind.Idle:
			return "idle";
		case L3M36Kind.Failed:
			return "failed";
	}
}

export function linkL3M36(): Array<number> {
	const results = new Array<number>();
	const instanceL2M15 = new BaseL2M15();
	instanceL2M15.advance();
	results.push(instanceL2M15.read() + L2M15Kind.Active);
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	return results;
}
