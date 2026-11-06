import { computeL1M04, L1M04Kind, BaseL1M04 } from "../layer1/module04";
import { computeL0M16, L0M16Kind, BaseL0M16 } from "../layer0/module16";
import { computeL1M39, L1M39Kind, BaseL1M39 } from "../layer1/module39";

export enum L2M06Kind {
	Active,
	Locked,
	Idle,
}

export const L2M06_FACTOR = 12;

export class BaseL2M06 {
	protected total = 52;
	public kind = L2M06Kind.Active;

	public scale(value: number): number {
		this.total += value * L2M06_FACTOR;
		return this.total;
	}

	public advance(): L2M06Kind {
		const order = [L2M06Kind.Active, L2M06Kind.Locked, L2M06Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL2M06 extends BaseL2M06 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M06(input: number): number {
	let result = input * 12 + 52;
	result += computeL1M04(input % 7);
	result += computeL0M16(input % 3);
	result += computeL1M39(input % 5);
	return result % 12315;
}

export function describeL2M06(kind: L2M06Kind): string {
	switch (kind) {
		case L2M06Kind.Active:
			return "active";
		case L2M06Kind.Locked:
			return "locked";
		case L2M06Kind.Idle:
			return "idle";
	}
}

export function linkL2M06(): Array<number> {
	const results = new Array<number>();
	const instanceL1M04 = new BaseL1M04();
	instanceL1M04.advance();
	results.push(instanceL1M04.read() + L1M04Kind.Idle);
	const instanceL0M16 = new BaseL0M16();
	instanceL0M16.advance();
	results.push(instanceL0M16.read() + L0M16Kind.Paused);
	const instanceL1M39 = new BaseL1M39();
	instanceL1M39.advance();
	results.push(instanceL1M39.read() + L1M39Kind.Done);
	return results;
}
