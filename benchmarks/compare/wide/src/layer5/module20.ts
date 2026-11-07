import { computeL4M25, L4M25Kind, BaseL4M25 } from "../layer4/module25";
import { computeL4M22, L4M22Kind, BaseL4M22 } from "../layer4/module22";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L5M20Kind {
	Locked,
	Done,
	Active,
	Idle,
}

export const L5M20_FACTOR = 13;

export class BaseL5M20 {
	protected total = 4;
	public kind = L5M20Kind.Locked;

	public blend(value: number): number {
		this.total += value * L5M20_FACTOR;
		return this.total;
	}

	public advance(): L5M20Kind {
		const order = [L5M20Kind.Locked, L5M20Kind.Done, L5M20Kind.Active, L5M20Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL5M20 extends BaseL5M20 {
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

export function computeL5M20(input: number): number {
	let result = input * 13 + 4;
	result += computeL4M25(input % 6);
	result += computeL4M22(input % 11);
	result += computeL0M01(input % 8);
	return result % 82660;
}

export function describeL5M20(kind: L5M20Kind): string {
	switch (kind) {
		case L5M20Kind.Locked:
			return "locked";
		case L5M20Kind.Done:
			return "done";
		case L5M20Kind.Active:
			return "active";
		case L5M20Kind.Idle:
			return "idle";
	}
}

export function linkL5M20(): Array<number> {
	const results = new Array<number>();
	const instanceL4M25 = new BaseL4M25();
	instanceL4M25.advance();
	results.push(instanceL4M25.read() + L4M25Kind.Idle);
	const instanceL4M22 = new BaseL4M22();
	instanceL4M22.advance();
	results.push(instanceL4M22.read() + L4M22Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
