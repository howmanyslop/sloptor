import { computeL7M27, L7M27Kind, BaseL7M27 } from "../layer7/module27";
import { computeL7M02, L7M02Kind, BaseL7M02 } from "../layer7/module02";

export enum L8M37Kind {
	Paused,
	Locked,
	Active,
	Failed,
}

export const L8M37_FACTOR = 9;

export class BaseL8M37 {
	protected total = 6;
	public kind = L8M37Kind.Paused;

	public mix(value: number): number {
		this.total += value * L8M37_FACTOR;
		return this.total;
	}

	public advance(): L8M37Kind {
		const order = [L8M37Kind.Paused, L8M37Kind.Locked, L8M37Kind.Active, L8M37Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M37(input: number): number {
	let result = input * 9 + 6;
	result += computeL7M27(input % 11);
	result += computeL7M02(input % 9);
	return result % 80196;
}

export const L8M37_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 6,
	chain: (value) => computeL8M37(value),
};

export function linkL8M37(): Array<number> {
	const results = new Array<number>();
	const instanceL7M27 = new BaseL7M27();
	instanceL7M27.advance();
	results.push(instanceL7M27.read() + L7M27Kind.Queued);
	const instanceL7M02 = new BaseL7M02();
	instanceL7M02.advance();
	results.push(instanceL7M02.read() + L7M02Kind.Queued);
	return results;
}
