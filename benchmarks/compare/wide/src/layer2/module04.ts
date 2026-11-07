import { computeL1M30, L1M30Kind, BaseL1M30 } from "../layer1/module30";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L2M04Kind {
	Paused,
	Locked,
	Active,
	Done,
}

export const L2M04_FACTOR = 14;

export class BaseL2M04 {
	protected total = 10;
	public kind = L2M04Kind.Paused;

	public warp(value: number): number {
		this.total += value * L2M04_FACTOR;
		return this.total;
	}

	public advance(): L2M04Kind {
		const order = [L2M04Kind.Paused, L2M04Kind.Locked, L2M04Kind.Active, L2M04Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M04(input: number): number {
	let result = input * 14 + 10;
	result += computeL1M30(input % 5);
	result += computeL0M03(input % 9);
	return result % 49328;
}

export function tableL2M04(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M04(index));
	return lookup;
}

export function linkL2M04(): Array<number> {
	const results = new Array<number>();
	const instanceL1M30 = new BaseL1M30();
	instanceL1M30.advance();
	results.push(instanceL1M30.read() + L1M30Kind.Locked);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
