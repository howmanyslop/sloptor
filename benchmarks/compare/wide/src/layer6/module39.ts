import { computeL5M39, L5M39Kind, BaseL5M39 } from "../layer5/module39";
import { computeL4M02, L4M02Kind, BaseL4M02 } from "../layer4/module02";
import { computeL1M27, L1M27Kind, BaseL1M27 } from "../layer1/module27";

export enum L6M39Kind {
	Done,
	Idle,
	Active,
}

export const L6M39_FACTOR = 16;

export class BaseL6M39 {
	protected total = 7;
	public kind = L6M39Kind.Done;

	public weigh(value: number): number {
		this.total += value * L6M39_FACTOR;
		return this.total;
	}

	public advance(): L6M39Kind {
		const order = [L6M39Kind.Done, L6M39Kind.Idle, L6M39Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL6M39 extends BaseL5M39 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M39(input: number): number {
	let result = input * 16 + 7;
	result += computeL5M39(input % 6);
	result += computeL4M02(input % 11);
	result += computeL1M27(input % 6);
	return result % 2680;
}

export function describeL6M39(kind: L6M39Kind): string {
	switch (kind) {
		case L6M39Kind.Done:
			return "done";
		case L6M39Kind.Idle:
			return "idle";
		case L6M39Kind.Active:
			return "active";
	}
}

export function linkL6M39(): Array<number> {
	const results = new Array<number>();
	const instanceL5M39 = new BaseL5M39();
	instanceL5M39.advance();
	results.push(instanceL5M39.read() + L5M39Kind.Done);
	const instanceL4M02 = new BaseL4M02();
	instanceL4M02.advance();
	results.push(instanceL4M02.read() + L4M02Kind.Queued);
	const instanceL1M27 = new BaseL1M27();
	instanceL1M27.advance();
	results.push(instanceL1M27.read() + L1M27Kind.Paused);
	return results;
}
