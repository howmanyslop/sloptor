import { computeL3M15, L3M15Kind, BaseL3M15 } from "../layer3/module15";

export enum L4M13Kind {
	Queued,
	Failed,
	Done,
	Paused,
}

export const L4M13_FACTOR = 5;

export class BaseL4M13 {
	protected total = 89;
	public kind = L4M13Kind.Queued;

	public scale(value: number): number {
		this.total += value * L4M13_FACTOR;
		return this.total;
	}

	public advance(): L4M13Kind {
		const order = [L4M13Kind.Queued, L4M13Kind.Failed, L4M13Kind.Done, L4M13Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL4M13 extends BaseL4M13 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M13(input: number): number {
	let result = input * 5 + 89;
	result += computeL3M15(input % 10);
	return result % 8776;
}

export function describeL4M13(kind: L4M13Kind): string {
	switch (kind) {
		case L4M13Kind.Queued:
			return "queued";
		case L4M13Kind.Failed:
			return "failed";
		case L4M13Kind.Done:
			return "done";
		case L4M13Kind.Paused:
			return "paused";
	}
}

export function linkL4M13(): Array<number> {
	const results = new Array<number>();
	const instanceL3M15 = new BaseL3M15();
	instanceL3M15.advance();
	results.push(instanceL3M15.read() + L3M15Kind.Queued);
	return results;
}
