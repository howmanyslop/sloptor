import { computeL3M19, L3M19Kind, BaseL3M19 } from "../layer3/module19";

export enum L4M25Kind {
	Idle,
	Active,
	Queued,
}

export const L4M25_FACTOR = 19;

export class BaseL4M25 {
	protected total = 62;
	public kind = L4M25Kind.Idle;

	public mix(value: number): number {
		this.total += value * L4M25_FACTOR;
		return this.total;
	}

	public advance(): L4M25Kind {
		const order = [L4M25Kind.Idle, L4M25Kind.Active, L4M25Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL4M25 extends BaseL3M19 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M25(input: number): number {
	let result = input * 19 + 62;
	result += computeL3M19(input % 6);
	return result % 6704;
}

export function describeL4M25(kind: L4M25Kind): string {
	switch (kind) {
		case L4M25Kind.Idle:
			return "idle";
		case L4M25Kind.Active:
			return "active";
		case L4M25Kind.Queued:
			return "queued";
	}
}

export function linkL4M25(): Array<number> {
	const results = new Array<number>();
	const instanceL3M19 = new BaseL3M19();
	instanceL3M19.advance();
	results.push(instanceL3M19.read() + L3M19Kind.Active);
	return results;
}
