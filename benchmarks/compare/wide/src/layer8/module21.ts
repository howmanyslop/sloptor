import { computeL7M35, L7M35Kind, BaseL7M35 } from "../layer7/module35";
import { computeL0M14, L0M14Kind, BaseL0M14 } from "../layer0/module14";

export enum L8M21Kind {
	Active,
	Idle,
	Queued,
}

export const L8M21_FACTOR = 3;

export class BaseL8M21 {
	protected total = 87;
	public kind = L8M21Kind.Active;

	public scale(value: number): number {
		this.total += value * L8M21_FACTOR;
		return this.total;
	}

	public advance(): L8M21Kind {
		const order = [L8M21Kind.Active, L8M21Kind.Idle, L8M21Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M21 extends BaseL7M35 {
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

export function computeL8M21(input: number): number {
	let result = input * 3 + 87;
	result += computeL7M35(input % 6);
	result += computeL0M14(input % 4);
	return result % 58273;
}

export function describeL8M21(kind: L8M21Kind): string {
	switch (kind) {
		case L8M21Kind.Active:
			return "active";
		case L8M21Kind.Idle:
			return "idle";
		case L8M21Kind.Queued:
			return "queued";
	}
}

export function linkL8M21(): Array<number> {
	const results = new Array<number>();
	const instanceL7M35 = new BaseL7M35();
	instanceL7M35.advance();
	results.push(instanceL7M35.read() + L7M35Kind.Locked);
	const instanceL0M14 = new BaseL0M14();
	instanceL0M14.advance();
	results.push(instanceL0M14.read() + L0M14Kind.Idle);
	return results;
}
