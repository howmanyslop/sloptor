import { computeL0M20, L0M20Kind, BaseL0M20 } from "../layer0/module20";
import { computeL0M12, L0M12Kind, BaseL0M12 } from "../layer0/module12";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M37Kind {
	Locked,
	Idle,
	Paused,
	Active,
	Done,
}

export const L1M37_FACTOR = 6;

export class BaseL1M37 {
	protected total = 67;
	public kind = L1M37Kind.Locked;

	public fold(value: number): number {
		this.total += value * L1M37_FACTOR;
		return this.total;
	}

	public advance(): L1M37Kind {
		const order = [L1M37Kind.Locked, L1M37Kind.Idle, L1M37Kind.Paused, L1M37Kind.Active, L1M37Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL1M37 extends BaseL1M37 {
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

export function computeL1M37(input: number): number {
	let result = input * 6 + 67;
	result += computeL0M20(input % 10);
	result += computeL0M12(input % 5);
	result += computeL0M06(input % 8);
	result += computeL0M02(input % 7);
	return result % 9532;
}

export function tableL1M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M37(index));
	return lookup;
}

export function linkL1M37(): Array<number> {
	const results = new Array<number>();
	const instanceL0M20 = new BaseL0M20();
	instanceL0M20.advance();
	results.push(instanceL0M20.read() + L0M20Kind.Failed);
	const instanceL0M12 = new BaseL0M12();
	instanceL0M12.advance();
	results.push(instanceL0M12.read() + L0M12Kind.Done);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
