import { computeL0M12, L0M12Kind, BaseL0M12 } from "../layer0/module12";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L1M13Kind {
	Idle,
	Locked,
	Active,
	Paused,
}

export const L1M13_FACTOR = 15;

export class BaseL1M13 {
	protected total = 41;
	public kind = L1M13Kind.Idle;

	public mix(value: number): number {
		this.total += value * L1M13_FACTOR;
		return this.total;
	}

	public advance(): L1M13Kind {
		const order = [L1M13Kind.Idle, L1M13Kind.Locked, L1M13Kind.Active, L1M13Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL1M13 extends BaseL0M12 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M13(input: number): number {
	let result = input * 15 + 41;
	result += computeL0M12(input % 8);
	result += computeL0M06(input % 3);
	result += computeL0M03(input % 4);
	return result % 1100;
}

export function describeL1M13(kind: L1M13Kind): string {
	switch (kind) {
		case L1M13Kind.Idle:
			return "idle";
		case L1M13Kind.Locked:
			return "locked";
		case L1M13Kind.Active:
			return "active";
		case L1M13Kind.Paused:
			return "paused";
	}
}

export function linkL1M13(): Array<number> {
	const results = new Array<number>();
	const instanceL0M12 = new BaseL0M12();
	instanceL0M12.advance();
	results.push(instanceL0M12.read() + L0M12Kind.Done);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
