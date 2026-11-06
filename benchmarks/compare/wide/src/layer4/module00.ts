import { computeL3M04, L3M04Kind, BaseL3M04 } from "../layer3/module04";
import { computeL2M11, L2M11Kind, BaseL2M11 } from "../layer2/module11";

export enum L4M00Kind {
	Idle,
	Active,
	Queued,
	Paused,
}

export const L4M00_FACTOR = 6;

export class BaseL4M00 {
	protected total = 31;
	public kind = L4M00Kind.Idle;

	public mix(value: number): number {
		this.total += value * L4M00_FACTOR;
		return this.total;
	}

	public advance(): L4M00Kind {
		const order = [L4M00Kind.Idle, L4M00Kind.Active, L4M00Kind.Queued, L4M00Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL4M00 extends BaseL3M04 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M00(input: number): number {
	let result = input * 6 + 31;
	result += computeL3M04(input % 5);
	result += computeL2M11(input % 6);
	return result % 3577;
}

export function describeL4M00(kind: L4M00Kind): string {
	switch (kind) {
		case L4M00Kind.Idle:
			return "idle";
		case L4M00Kind.Active:
			return "active";
		case L4M00Kind.Queued:
			return "queued";
		case L4M00Kind.Paused:
			return "paused";
	}
}

export function linkL4M00(): Array<number> {
	const results = new Array<number>();
	const instanceL3M04 = new BaseL3M04();
	instanceL3M04.advance();
	results.push(instanceL3M04.read() + L3M04Kind.Paused);
	const instanceL2M11 = new BaseL2M11();
	instanceL2M11.advance();
	results.push(instanceL2M11.read() + L2M11Kind.Paused);
	return results;
}
