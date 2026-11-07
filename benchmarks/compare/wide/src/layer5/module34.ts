import { computeL4M03, L4M03Kind, BaseL4M03 } from "../layer4/module03";
import { computeL1M09, L1M09Kind, BaseL1M09 } from "../layer1/module09";
import { computeL2M15, L2M15Kind, BaseL2M15 } from "../layer2/module15";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L5M34Kind {
	Idle,
	Active,
	Locked,
}

export const L5M34_FACTOR = 10;

export class BaseL5M34 {
	protected total = 89;
	public kind = L5M34Kind.Idle;

	public shift(value: number): number {
		this.total += value * L5M34_FACTOR;
		return this.total;
	}

	public advance(): L5M34Kind {
		const order = [L5M34Kind.Idle, L5M34Kind.Active, L5M34Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL5M34 extends BaseL5M34 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M34(input: number): number {
	let result = input * 10 + 89;
	result += computeL4M03(input % 10);
	result += computeL1M09(input % 11);
	result += computeL2M15(input % 9);
	result += computeL0M01(input % 4);
	return result % 58906;
}

export const L5M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 89,
	chain: (value) => computeL5M34(value),
};

export function linkL5M34(): Array<number> {
	const results = new Array<number>();
	const instanceL4M03 = new BaseL4M03();
	instanceL4M03.advance();
	results.push(instanceL4M03.read() + L4M03Kind.Paused);
	const instanceL1M09 = new BaseL1M09();
	instanceL1M09.advance();
	results.push(instanceL1M09.read() + L1M09Kind.Idle);
	const instanceL2M15 = new BaseL2M15();
	instanceL2M15.advance();
	results.push(instanceL2M15.read() + L2M15Kind.Active);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
