import { computeL2M19, L2M19Kind, BaseL2M19 } from "../layer2/module19";
import { computeL0M07, L0M07Kind, BaseL0M07 } from "../layer0/module07";
import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M08Kind {
	Paused,
	Active,
	Done,
	Idle,
	Locked,
}

export const L3M08_FACTOR = 8;

export class BaseL3M08 {
	protected total = 82;
	public kind = L3M08Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L3M08_FACTOR;
		return this.total;
	}

	public advance(): L3M08Kind {
		const order = [L3M08Kind.Paused, L3M08Kind.Active, L3M08Kind.Done, L3M08Kind.Idle, L3M08Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL3M08 extends BaseL3M08 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M08(input: number): number {
	let result = input * 8 + 82;
	result += computeL2M19(input % 6);
	result += computeL0M07(input % 4);
	result += computeL0M27(input % 8);
	result += computeL0M01(input % 5);
	return result % 57846;
}

export function tableL3M08(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M08(index));
	return lookup;
}

export function linkL3M08(): Array<number> {
	const results = new Array<number>();
	const instanceL2M19 = new BaseL2M19();
	instanceL2M19.advance();
	results.push(instanceL2M19.read() + L2M19Kind.Active);
	const instanceL0M07 = new BaseL0M07();
	instanceL0M07.advance();
	results.push(instanceL0M07.read() + L0M07Kind.Idle);
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
