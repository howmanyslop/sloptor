import { computeL2M25, L2M25Kind, BaseL2M25 } from "../layer2/module25";
import { computeL0M11, L0M11Kind, BaseL0M11 } from "../layer0/module11";

export enum L3M26Kind {
	Failed,
	Locked,
	Queued,
	Active,
	Idle,
}

export const L3M26_FACTOR = 13;

export class BaseL3M26 {
	protected total = 93;
	public kind = L3M26Kind.Failed;

	public mix(value: number): number {
		this.total += value * L3M26_FACTOR;
		return this.total;
	}

	public advance(): L3M26Kind {
		const order = [L3M26Kind.Failed, L3M26Kind.Locked, L3M26Kind.Queued, L3M26Kind.Active, L3M26Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL3M26 extends BaseL2M25 {
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

export function computeL3M26(input: number): number {
	let result = input * 13 + 93;
	result += computeL2M25(input % 11);
	result += computeL0M11(input % 11);
	return result % 75554;
}

export function describeL3M26(kind: L3M26Kind): string {
	switch (kind) {
		case L3M26Kind.Failed:
			return "failed";
		case L3M26Kind.Locked:
			return "locked";
		case L3M26Kind.Queued:
			return "queued";
		case L3M26Kind.Active:
			return "active";
		case L3M26Kind.Idle:
			return "idle";
	}
}

export function linkL3M26(): Array<number> {
	const results = new Array<number>();
	const instanceL2M25 = new BaseL2M25();
	instanceL2M25.advance();
	results.push(instanceL2M25.read() + L2M25Kind.Idle);
	const instanceL0M11 = new BaseL0M11();
	instanceL0M11.advance();
	results.push(instanceL0M11.read() + L0M11Kind.Active);
	return results;
}
