import { computeL3M02, L3M02Kind, BaseL3M02 } from "../layer3/module02";
import { computeL3M35, L3M35Kind, BaseL3M35 } from "../layer3/module35";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L4M34Kind {
	Paused,
	Done,
	Active,
}

export const L4M34_FACTOR = 6;

export class BaseL4M34 {
	protected total = 15;
	public kind = L4M34Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L4M34_FACTOR;
		return this.total;
	}

	public advance(): L4M34Kind {
		const order = [L4M34Kind.Paused, L4M34Kind.Done, L4M34Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL4M34 extends BaseL4M34 {
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

export function computeL4M34(input: number): number {
	let result = input * 6 + 15;
	result += computeL3M02(input % 9);
	result += computeL3M35(input % 5);
	result += computeL0M03(input % 8);
	return result % 79661;
}

export function describeL4M34(kind: L4M34Kind): string {
	switch (kind) {
		case L4M34Kind.Paused:
			return "paused";
		case L4M34Kind.Done:
			return "done";
		case L4M34Kind.Active:
			return "active";
	}
}

export function linkL4M34(): Array<number> {
	const results = new Array<number>();
	const instanceL3M02 = new BaseL3M02();
	instanceL3M02.advance();
	results.push(instanceL3M02.read() + L3M02Kind.Failed);
	const instanceL3M35 = new BaseL3M35();
	instanceL3M35.advance();
	results.push(instanceL3M35.read() + L3M35Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
