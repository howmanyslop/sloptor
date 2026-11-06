import { computeL5M33, L5M33Kind, BaseL5M33 } from "../layer5/module33";
import { computeL4M05, L4M05Kind, BaseL4M05 } from "../layer4/module05";
import { computeL5M37, L5M37Kind, BaseL5M37 } from "../layer5/module37";

export enum L6M21Kind {
	Queued,
	Idle,
	Active,
	Paused,
	Done,
}

export const L6M21_FACTOR = 9;

export class BaseL6M21 {
	protected total = 74;
	public kind = L6M21Kind.Queued;

	public scale(value: number): number {
		this.total += value * L6M21_FACTOR;
		return this.total;
	}

	public advance(): L6M21Kind {
		const order = [L6M21Kind.Queued, L6M21Kind.Idle, L6M21Kind.Active, L6M21Kind.Paused, L6M21Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL6M21 extends BaseL6M21 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M21(input: number): number {
	let result = input * 9 + 74;
	result += computeL5M33(input % 7);
	result += computeL4M05(input % 9);
	result += computeL5M37(input % 8);
	return result % 66394;
}

export function describeL6M21(kind: L6M21Kind): string {
	switch (kind) {
		case L6M21Kind.Queued:
			return "queued";
		case L6M21Kind.Idle:
			return "idle";
		case L6M21Kind.Active:
			return "active";
		case L6M21Kind.Paused:
			return "paused";
		case L6M21Kind.Done:
			return "done";
	}
}

export function linkL6M21(): Array<number> {
	const results = new Array<number>();
	const instanceL5M33 = new BaseL5M33();
	instanceL5M33.advance();
	results.push(instanceL5M33.read() + L5M33Kind.Idle);
	const instanceL4M05 = new BaseL4M05();
	instanceL4M05.advance();
	results.push(instanceL4M05.read() + L4M05Kind.Idle);
	const instanceL5M37 = new BaseL5M37();
	instanceL5M37.advance();
	results.push(instanceL5M37.read() + L5M37Kind.Paused);
	return results;
}
