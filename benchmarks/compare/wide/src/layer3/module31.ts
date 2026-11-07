import { computeL2M20, L2M20Kind, BaseL2M20 } from "../layer2/module20";
import { computeL2M16, L2M16Kind, BaseL2M16 } from "../layer2/module16";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L3M31Kind {
	Idle,
	Queued,
	Locked,
	Active,
	Paused,
}

export const L3M31_FACTOR = 10;

export class BaseL3M31 {
	protected total = 27;
	public kind = L3M31Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L3M31_FACTOR;
		return this.total;
	}

	public advance(): L3M31Kind {
		const order = [L3M31Kind.Idle, L3M31Kind.Queued, L3M31Kind.Locked, L3M31Kind.Active, L3M31Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL3M31 extends BaseL2M20 {
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

export function computeL3M31(input: number): number {
	let result = input * 10 + 27;
	result += computeL2M20(input % 7);
	result += computeL2M16(input % 11);
	result += computeL0M03(input % 5);
	return result % 46342;
}

export function describeL3M31(kind: L3M31Kind): string {
	switch (kind) {
		case L3M31Kind.Idle:
			return "idle";
		case L3M31Kind.Queued:
			return "queued";
		case L3M31Kind.Locked:
			return "locked";
		case L3M31Kind.Active:
			return "active";
		case L3M31Kind.Paused:
			return "paused";
	}
}

export function linkL3M31(): Array<number> {
	const results = new Array<number>();
	const instanceL2M20 = new BaseL2M20();
	instanceL2M20.advance();
	results.push(instanceL2M20.read() + L2M20Kind.Done);
	const instanceL2M16 = new BaseL2M16();
	instanceL2M16.advance();
	results.push(instanceL2M16.read() + L2M16Kind.Paused);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
