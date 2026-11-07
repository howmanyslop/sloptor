import { computeL7M13, L7M13Kind, BaseL7M13 } from "../layer7/module13";
import { computeL4M26, L4M26Kind, BaseL4M26 } from "../layer4/module26";

export enum L8M35Kind {
	Idle,
	Active,
	Queued,
}

export const L8M35_FACTOR = 5;

export class BaseL8M35 {
	protected total = 80;
	public kind = L8M35Kind.Idle;

	public fold(value: number): number {
		this.total += value * L8M35_FACTOR;
		return this.total;
	}

	public advance(): L8M35Kind {
		const order = [L8M35Kind.Idle, L8M35Kind.Active, L8M35Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL8M35 extends BaseL7M13 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M35(input: number): number {
	let result = input * 5 + 80;
	result += computeL7M13(input % 5);
	result += computeL4M26(input % 4);
	return result % 31945;
}

export function describeL8M35(kind: L8M35Kind): string {
	switch (kind) {
		case L8M35Kind.Idle:
			return "idle";
		case L8M35Kind.Active:
			return "active";
		case L8M35Kind.Queued:
			return "queued";
	}
}

export function linkL8M35(): Array<number> {
	const results = new Array<number>();
	const instanceL7M13 = new BaseL7M13();
	instanceL7M13.advance();
	results.push(instanceL7M13.read() + L7M13Kind.Done);
	const instanceL4M26 = new BaseL4M26();
	instanceL4M26.advance();
	results.push(instanceL4M26.read() + L4M26Kind.Active);
	return results;
}
