import { computeL7M21, L7M21Kind, BaseL7M21 } from "../layer7/module21";
import { computeL4M23, L4M23Kind, BaseL4M23 } from "../layer4/module23";
import { computeL2M27, L2M27Kind, BaseL2M27 } from "../layer2/module27";

export enum L8M19Kind {
	Failed,
	Done,
	Paused,
	Active,
}

export const L8M19_FACTOR = 5;

export class BaseL8M19 {
	protected total = 65;
	public kind = L8M19Kind.Failed;

	public tally(value: number): number {
		this.total += value * L8M19_FACTOR;
		return this.total;
	}

	public advance(): L8M19Kind {
		const order = [L8M19Kind.Failed, L8M19Kind.Done, L8M19Kind.Paused, L8M19Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M19(input: number): number {
	let result = input * 5 + 65;
	result += computeL7M21(input % 10);
	result += computeL4M23(input % 5);
	result += computeL2M27(input % 8);
	return result % 20206;
}

export function tableL8M19(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M19(index));
	return lookup;
}

export function linkL8M19(): Array<number> {
	const results = new Array<number>();
	const instanceL7M21 = new BaseL7M21();
	instanceL7M21.advance();
	results.push(instanceL7M21.read() + L7M21Kind.Locked);
	const instanceL4M23 = new BaseL4M23();
	instanceL4M23.advance();
	results.push(instanceL4M23.read() + L4M23Kind.Active);
	const instanceL2M27 = new BaseL2M27();
	instanceL2M27.advance();
	results.push(instanceL2M27.read() + L2M27Kind.Failed);
	return results;
}
