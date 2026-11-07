import { computeL7M20, L7M20Kind, BaseL7M20 } from "../layer7/module20";
import { computeL4M28, L4M28Kind, BaseL4M28 } from "../layer4/module28";
import { computeL7M11, L7M11Kind, BaseL7M11 } from "../layer7/module11";

export enum L8M36Kind {
	Paused,
	Locked,
	Idle,
}

export const L8M36_FACTOR = 19;

export class BaseL8M36 {
	protected total = 61;
	public kind = L8M36Kind.Paused;

	public mix(value: number): number {
		this.total += value * L8M36_FACTOR;
		return this.total;
	}

	public advance(): L8M36Kind {
		const order = [L8M36Kind.Paused, L8M36Kind.Locked, L8M36Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M36(input: number): number {
	let result = input * 19 + 61;
	result += computeL7M20(input % 7);
	result += computeL4M28(input % 9);
	result += computeL7M11(input % 5);
	return result % 56913;
}

export const L8M36_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 61,
	chain: (value) => computeL8M36(value),
};

export function linkL8M36(): Array<number> {
	const results = new Array<number>();
	const instanceL7M20 = new BaseL7M20();
	instanceL7M20.advance();
	results.push(instanceL7M20.read() + L7M20Kind.Failed);
	const instanceL4M28 = new BaseL4M28();
	instanceL4M28.advance();
	results.push(instanceL4M28.read() + L4M28Kind.Locked);
	const instanceL7M11 = new BaseL7M11();
	instanceL7M11.advance();
	results.push(instanceL7M11.read() + L7M11Kind.Done);
	return results;
}
