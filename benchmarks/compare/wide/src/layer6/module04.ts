import { computeL5M34, L5M34Kind, BaseL5M34 } from "../layer5/module34";
import { computeL5M37, L5M37Kind, BaseL5M37 } from "../layer5/module37";
import { computeL2M15, L2M15Kind, BaseL2M15 } from "../layer2/module15";

export enum L6M04Kind {
	Locked,
	Failed,
	Done,
}

export const L6M04_FACTOR = 14;

export class BaseL6M04 {
	protected total = 77;
	public kind = L6M04Kind.Locked;

	public warp(value: number): number {
		this.total += value * L6M04_FACTOR;
		return this.total;
	}

	public advance(): L6M04Kind {
		const order = [L6M04Kind.Locked, L6M04Kind.Failed, L6M04Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL6M04 extends BaseL5M34 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M04(input: number): number {
	let result = input * 14 + 77;
	result += computeL5M34(input % 8);
	result += computeL5M37(input % 5);
	result += computeL2M15(input % 11);
	return result % 34303;
}

export const L6M04_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 77,
	chain: (value) => computeL6M04(value),
};

export function linkL6M04(): Array<number> {
	const results = new Array<number>();
	const instanceL5M34 = new BaseL5M34();
	instanceL5M34.advance();
	results.push(instanceL5M34.read() + L5M34Kind.Idle);
	const instanceL5M37 = new BaseL5M37();
	instanceL5M37.advance();
	results.push(instanceL5M37.read() + L5M37Kind.Paused);
	const instanceL2M15 = new BaseL2M15();
	instanceL2M15.advance();
	results.push(instanceL2M15.read() + L2M15Kind.Active);
	return results;
}
