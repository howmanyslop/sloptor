import { computeL7M28, L7M28Kind, BaseL7M28 } from "../layer7/module28";
import { computeL2M06, L2M06Kind, BaseL2M06 } from "../layer2/module06";
import { computeL3M24, L3M24Kind, BaseL3M24 } from "../layer3/module24";

export enum L8M20Kind {
	Done,
	Paused,
	Failed,
	Active,
	Queued,
}

export const L8M20_FACTOR = 7;

export class BaseL8M20 {
	protected total = 97;
	public kind = L8M20Kind.Done;

	public blend(value: number): number {
		this.total += value * L8M20_FACTOR;
		return this.total;
	}

	public advance(): L8M20Kind {
		const order = [L8M20Kind.Done, L8M20Kind.Paused, L8M20Kind.Failed, L8M20Kind.Active, L8M20Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M20(input: number): number {
	let result = input * 7 + 97;
	result += computeL7M28(input % 8);
	result += computeL2M06(input % 7);
	result += computeL3M24(input % 10);
	return result % 20649;
}

export function tableL8M20(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M20(index));
	return lookup;
}

export function linkL8M20(): Array<number> {
	const results = new Array<number>();
	const instanceL7M28 = new BaseL7M28();
	instanceL7M28.advance();
	results.push(instanceL7M28.read() + L7M28Kind.Queued);
	const instanceL2M06 = new BaseL2M06();
	instanceL2M06.advance();
	results.push(instanceL2M06.read() + L2M06Kind.Active);
	const instanceL3M24 = new BaseL3M24();
	instanceL3M24.advance();
	results.push(instanceL3M24.read() + L3M24Kind.Idle);
	return results;
}
