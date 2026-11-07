import { computeL5M11, L5M11Kind, BaseL5M11 } from "../layer5/module11";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L6M35Kind {
	Locked,
	Idle,
	Active,
	Paused,
}

export const L6M35_FACTOR = 17;

export class BaseL6M35 {
	protected total = 77;
	public kind = L6M35Kind.Locked;

	public mix(value: number): number {
		this.total += value * L6M35_FACTOR;
		return this.total;
	}

	public advance(): L6M35Kind {
		const order = [L6M35Kind.Locked, L6M35Kind.Idle, L6M35Kind.Active, L6M35Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M35(input: number): number {
	let result = input * 17 + 77;
	result += computeL5M11(input % 4);
	result += computeL0M00(input % 11);
	return result % 96803;
}

export function tableL6M35(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M35(index));
	return lookup;
}

export function linkL6M35(): Array<number> {
	const results = new Array<number>();
	const instanceL5M11 = new BaseL5M11();
	instanceL5M11.advance();
	results.push(instanceL5M11.read() + L5M11Kind.Active);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
