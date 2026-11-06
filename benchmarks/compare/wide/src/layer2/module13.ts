import { computeL1M13, L1M13Kind, BaseL1M13 } from "../layer1/module13";
import { computeL1M24, L1M24Kind, BaseL1M24 } from "../layer1/module24";

export enum L2M13Kind {
	Failed,
	Idle,
	Queued,
	Locked,
	Active,
}

export const L2M13_FACTOR = 14;

export class BaseL2M13 {
	protected total = 91;
	public kind = L2M13Kind.Failed;

	public mix(value: number): number {
		this.total += value * L2M13_FACTOR;
		return this.total;
	}

	public advance(): L2M13Kind {
		const order = [L2M13Kind.Failed, L2M13Kind.Idle, L2M13Kind.Queued, L2M13Kind.Locked, L2M13Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL2M13 extends BaseL2M13 {
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

export function computeL2M13(input: number): number {
	let result = input * 14 + 91;
	result += computeL1M13(input % 11);
	result += computeL1M24(input % 11);
	return result % 8222;
}

export function describeL2M13(kind: L2M13Kind): string {
	switch (kind) {
		case L2M13Kind.Failed:
			return "failed";
		case L2M13Kind.Idle:
			return "idle";
		case L2M13Kind.Queued:
			return "queued";
		case L2M13Kind.Locked:
			return "locked";
		case L2M13Kind.Active:
			return "active";
	}
}

export function linkL2M13(): Array<number> {
	const results = new Array<number>();
	const instanceL1M13 = new BaseL1M13();
	instanceL1M13.advance();
	results.push(instanceL1M13.read() + L1M13Kind.Idle);
	const instanceL1M24 = new BaseL1M24();
	instanceL1M24.advance();
	results.push(instanceL1M24.read() + L1M24Kind.Active);
	return results;
}
