import { computeL4M08, L4M08Kind, BaseL4M08 } from "../layer4/module08";

export enum L5M29Kind {
	Paused,
	Active,
	Queued,
	Locked,
}

export const L5M29_FACTOR = 17;

export class BaseL5M29 {
	protected total = 75;
	public kind = L5M29Kind.Paused;

	public tally(value: number): number {
		this.total += value * L5M29_FACTOR;
		return this.total;
	}

	public advance(): L5M29Kind {
		const order = [L5M29Kind.Paused, L5M29Kind.Active, L5M29Kind.Queued, L5M29Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL5M29 extends BaseL4M08 {
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

export function computeL5M29(input: number): number {
	let result = input * 17 + 75;
	result += computeL4M08(input % 7);
	return result % 30578;
}

export function describeL5M29(kind: L5M29Kind): string {
	switch (kind) {
		case L5M29Kind.Paused:
			return "paused";
		case L5M29Kind.Active:
			return "active";
		case L5M29Kind.Queued:
			return "queued";
		case L5M29Kind.Locked:
			return "locked";
	}
}

export function linkL5M29(): Array<number> {
	const results = new Array<number>();
	const instanceL4M08 = new BaseL4M08();
	instanceL4M08.advance();
	results.push(instanceL4M08.read() + L4M08Kind.Idle);
	return results;
}
