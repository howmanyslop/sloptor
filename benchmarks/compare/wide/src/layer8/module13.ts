import { computeL7M19, L7M19Kind, BaseL7M19 } from "../layer7/module19";

export enum L8M13Kind {
	Done,
	Idle,
	Failed,
	Queued,
	Active,
}

export const L8M13_FACTOR = 2;

export class BaseL8M13 {
	protected total = 95;
	public kind = L8M13Kind.Done;

	public weigh(value: number): number {
		this.total += value * L8M13_FACTOR;
		return this.total;
	}

	public advance(): L8M13Kind {
		const order = [L8M13Kind.Done, L8M13Kind.Idle, L8M13Kind.Failed, L8M13Kind.Queued, L8M13Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL8M13 extends BaseL7M19 {
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

export function computeL8M13(input: number): number {
	let result = input * 2 + 95;
	result += computeL7M19(input % 11);
	return result % 62388;
}

export function describeL8M13(kind: L8M13Kind): string {
	switch (kind) {
		case L8M13Kind.Done:
			return "done";
		case L8M13Kind.Idle:
			return "idle";
		case L8M13Kind.Failed:
			return "failed";
		case L8M13Kind.Queued:
			return "queued";
		case L8M13Kind.Active:
			return "active";
	}
}

export function linkL8M13(): Array<number> {
	const results = new Array<number>();
	const instanceL7M19 = new BaseL7M19();
	instanceL7M19.advance();
	results.push(instanceL7M19.read() + L7M19Kind.Done);
	return results;
}
