import { computeL7M22, L7M22Kind, BaseL7M22 } from "../layer7/module22";
import { computeL5M34, L5M34Kind, BaseL5M34 } from "../layer5/module34";
import { computeL6M36, L6M36Kind, BaseL6M36 } from "../layer6/module36";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L8M02Kind {
	Paused,
	Done,
	Queued,
	Locked,
	Active,
}

export const L8M02_FACTOR = 3;

export class BaseL8M02 {
	protected total = 60;
	public kind = L8M02Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L8M02_FACTOR;
		return this.total;
	}

	public advance(): L8M02Kind {
		const order = [L8M02Kind.Paused, L8M02Kind.Done, L8M02Kind.Queued, L8M02Kind.Locked, L8M02Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL8M02 extends BaseL7M22 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M02(input: number): number {
	let result = input * 3 + 60;
	result += computeL7M22(input % 6);
	result += computeL5M34(input % 5);
	result += computeL6M36(input % 7);
	result += computeL0M03(input % 10);
	return result % 68949;
}

export const L8M02_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 60,
	chain: (value) => computeL8M02(value),
};

export function linkL8M02(): Array<number> {
	const results = new Array<number>();
	const instanceL7M22 = new BaseL7M22();
	instanceL7M22.advance();
	results.push(instanceL7M22.read() + L7M22Kind.Paused);
	const instanceL5M34 = new BaseL5M34();
	instanceL5M34.advance();
	results.push(instanceL5M34.read() + L5M34Kind.Idle);
	const instanceL6M36 = new BaseL6M36();
	instanceL6M36.advance();
	results.push(instanceL6M36.read() + L6M36Kind.Queued);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
