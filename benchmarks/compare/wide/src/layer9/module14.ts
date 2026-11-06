import { computeL8M27, L8M27Kind, BaseL8M27 } from "../layer8/module27";
import { computeL2M17, L2M17Kind, BaseL2M17 } from "../layer2/module17";

export enum L9M14Kind {
	Idle,
	Locked,
	Active,
}

export const L9M14_FACTOR = 15;

export class BaseL9M14 {
	protected total = 15;
	public kind = L9M14Kind.Idle;

	public blend(value: number): number {
		this.total += value * L9M14_FACTOR;
		return this.total;
	}

	public advance(): L9M14Kind {
		const order = [L9M14Kind.Idle, L9M14Kind.Locked, L9M14Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M14 extends BaseL8M27 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M14(input: number): number {
	let result = input * 15 + 15;
	result += computeL8M27(input % 9);
	result += computeL2M17(input % 3);
	return result % 44701;
}

export function describeL9M14(kind: L9M14Kind): string {
	switch (kind) {
		case L9M14Kind.Idle:
			return "idle";
		case L9M14Kind.Locked:
			return "locked";
		case L9M14Kind.Active:
			return "active";
	}
}

export function linkL9M14(): Array<number> {
	const results = new Array<number>();
	const instanceL8M27 = new BaseL8M27();
	instanceL8M27.advance();
	results.push(instanceL8M27.read() + L8M27Kind.Paused);
	const instanceL2M17 = new BaseL2M17();
	instanceL2M17.advance();
	results.push(instanceL2M17.read() + L2M17Kind.Idle);
	return results;
}
