import { computeL4M32, L4M32Kind, BaseL4M32 } from "../layer4/module32";
import { computeL3M05, L3M05Kind, BaseL3M05 } from "../layer3/module05";
import { computeL1M31, L1M31Kind, BaseL1M31 } from "../layer1/module31";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M21Kind {
	Idle,
	Done,
	Active,
}

export const L5M21_FACTOR = 17;

export class BaseL5M21 {
	protected total = 78;
	public kind = L5M21Kind.Idle;

	public tally(value: number): number {
		this.total += value * L5M21_FACTOR;
		return this.total;
	}

	public advance(): L5M21Kind {
		const order = [L5M21Kind.Idle, L5M21Kind.Done, L5M21Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M21(input: number): number {
	let result = input * 17 + 78;
	result += computeL4M32(input % 10);
	result += computeL3M05(input % 5);
	result += computeL1M31(input % 8);
	result += computeL0M00(input % 10);
	return result % 57396;
}

export function tableL5M21(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M21(index));
	return lookup;
}

export function linkL5M21(): Array<number> {
	const results = new Array<number>();
	const instanceL4M32 = new BaseL4M32();
	instanceL4M32.advance();
	results.push(instanceL4M32.read() + L4M32Kind.Paused);
	const instanceL3M05 = new BaseL3M05();
	instanceL3M05.advance();
	results.push(instanceL3M05.read() + L3M05Kind.Failed);
	const instanceL1M31 = new BaseL1M31();
	instanceL1M31.advance();
	results.push(instanceL1M31.read() + L1M31Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
