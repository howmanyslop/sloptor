import { computeL7M16, L7M16Kind, BaseL7M16 } from "../layer7/module16";
import { computeL7M08, L7M08Kind, BaseL7M08 } from "../layer7/module08";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L8M24Kind {
	Done,
	Active,
	Paused,
	Locked,
	Queued,
}

export const L8M24_FACTOR = 13;

export class BaseL8M24 {
	protected total = 88;
	public kind = L8M24Kind.Done;

	public mix(value: number): number {
		this.total += value * L8M24_FACTOR;
		return this.total;
	}

	public advance(): L8M24Kind {
		const order = [L8M24Kind.Done, L8M24Kind.Active, L8M24Kind.Paused, L8M24Kind.Locked, L8M24Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M24(input: number): number {
	let result = input * 13 + 88;
	result += computeL7M16(input % 7);
	result += computeL7M08(input % 8);
	result += computeL0M03(input % 7);
	return result % 93596;
}

export function tableL8M24(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M24(index));
	return lookup;
}

export function linkL8M24(): Array<number> {
	const results = new Array<number>();
	const instanceL7M16 = new BaseL7M16();
	instanceL7M16.advance();
	results.push(instanceL7M16.read() + L7M16Kind.Locked);
	const instanceL7M08 = new BaseL7M08();
	instanceL7M08.advance();
	results.push(instanceL7M08.read() + L7M08Kind.Paused);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
