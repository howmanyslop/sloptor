import { computeL1M19, L1M19Kind, BaseL1M19 } from "../layer1/module19";
import { computeL1M22, L1M22Kind, BaseL1M22 } from "../layer1/module22";
import { computeL1M17, L1M17Kind, BaseL1M17 } from "../layer1/module17";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M31Kind {
	Idle,
	Active,
	Failed,
	Queued,
}

export const L2M31_FACTOR = 8;

export class BaseL2M31 {
	protected total = 11;
	public kind = L2M31Kind.Idle;

	public mix(value: number): number {
		this.total += value * L2M31_FACTOR;
		return this.total;
	}

	public advance(): L2M31Kind {
		const order = [L2M31Kind.Idle, L2M31Kind.Active, L2M31Kind.Failed, L2M31Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL2M31 extends BaseL1M19 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M31(input: number): number {
	let result = input * 8 + 11;
	result += computeL1M19(input % 7);
	result += computeL1M22(input % 7);
	result += computeL1M17(input % 11);
	result += computeL0M02(input % 3);
	return result % 57487;
}

export const L2M31_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 11,
	chain: (value) => computeL2M31(value),
};

export function linkL2M31(): Array<number> {
	const results = new Array<number>();
	const instanceL1M19 = new BaseL1M19();
	instanceL1M19.advance();
	results.push(instanceL1M19.read() + L1M19Kind.Idle);
	const instanceL1M22 = new BaseL1M22();
	instanceL1M22.advance();
	results.push(instanceL1M22.read() + L1M22Kind.Idle);
	const instanceL1M17 = new BaseL1M17();
	instanceL1M17.advance();
	results.push(instanceL1M17.read() + L1M17Kind.Paused);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
