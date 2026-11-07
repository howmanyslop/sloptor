import { computeL3M09, L3M09Kind, BaseL3M09 } from "../layer3/module09";

export enum L4M35Kind {
	Queued,
	Active,
	Paused,
}

export const L4M35_FACTOR = 15;

export class BaseL4M35 {
	protected total = 48;
	public kind = L4M35Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L4M35_FACTOR;
		return this.total;
	}

	public advance(): L4M35Kind {
		const order = [L4M35Kind.Queued, L4M35Kind.Active, L4M35Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL4M35 extends BaseL3M09 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M35(input: number): number {
	let result = input * 15 + 48;
	result += computeL3M09(input % 5);
	return result % 61567;
}

export function describeL4M35(kind: L4M35Kind): string {
	switch (kind) {
		case L4M35Kind.Queued:
			return "queued";
		case L4M35Kind.Active:
			return "active";
		case L4M35Kind.Paused:
			return "paused";
	}
}

export function linkL4M35(): Array<number> {
	const results = new Array<number>();
	const instanceL3M09 = new BaseL3M09();
	instanceL3M09.advance();
	results.push(instanceL3M09.read() + L3M09Kind.Locked);
	return results;
}
