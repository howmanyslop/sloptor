import { computeL7M29, L7M29Kind, BaseL7M29 } from "../layer7/module29";
import { computeL3M31, L3M31Kind, BaseL3M31 } from "../layer3/module31";
import { computeL5M09, L5M09Kind, BaseL5M09 } from "../layer5/module09";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L8M03Kind {
	Queued,
	Paused,
	Done,
	Locked,
}

export const L8M03_FACTOR = 4;

export class BaseL8M03 {
	protected total = 27;
	public kind = L8M03Kind.Queued;

	public blend(value: number): number {
		this.total += value * L8M03_FACTOR;
		return this.total;
	}

	public advance(): L8M03Kind {
		const order = [L8M03Kind.Queued, L8M03Kind.Paused, L8M03Kind.Done, L8M03Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL8M03 extends BaseL7M29 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M03(input: number): number {
	let result = input * 4 + 27;
	result += computeL7M29(input % 7);
	result += computeL3M31(input % 11);
	result += computeL5M09(input % 6);
	result += computeL0M02(input % 6);
	return result % 53770;
}

export const L8M03_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 27,
	chain: (value) => computeL8M03(value),
};

export function linkL8M03(): Array<number> {
	const results = new Array<number>();
	const instanceL7M29 = new BaseL7M29();
	instanceL7M29.advance();
	results.push(instanceL7M29.read() + L7M29Kind.Active);
	const instanceL3M31 = new BaseL3M31();
	instanceL3M31.advance();
	results.push(instanceL3M31.read() + L3M31Kind.Idle);
	const instanceL5M09 = new BaseL5M09();
	instanceL5M09.advance();
	results.push(instanceL5M09.read() + L5M09Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
