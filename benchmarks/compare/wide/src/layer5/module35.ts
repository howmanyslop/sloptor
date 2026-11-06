import { computeL4M10, L4M10Kind, BaseL4M10 } from "../layer4/module10";
import { computeL2M21, L2M21Kind, BaseL2M21 } from "../layer2/module21";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M35Kind {
	Done,
	Idle,
	Paused,
	Active,
	Failed,
}

export const L5M35_FACTOR = 12;

export class BaseL5M35 {
	protected total = 57;
	public kind = L5M35Kind.Done;

	public weigh(value: number): number {
		this.total += value * L5M35_FACTOR;
		return this.total;
	}

	public advance(): L5M35Kind {
		const order = [L5M35Kind.Done, L5M35Kind.Idle, L5M35Kind.Paused, L5M35Kind.Active, L5M35Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M35(input: number): number {
	let result = input * 12 + 57;
	result += computeL4M10(input % 5);
	result += computeL2M21(input % 6);
	result += computeL0M00(input % 11);
	return result % 80585;
}

export function tableL5M35(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M35(index));
	return lookup;
}

export function linkL5M35(): Array<number> {
	const results = new Array<number>();
	const instanceL4M10 = new BaseL4M10();
	instanceL4M10.advance();
	results.push(instanceL4M10.read() + L4M10Kind.Active);
	const instanceL2M21 = new BaseL2M21();
	instanceL2M21.advance();
	results.push(instanceL2M21.read() + L2M21Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
