import { computeL3M31, L3M31Kind, BaseL3M31 } from "../layer3/module31";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L4M21Kind {
	Paused,
	Queued,
	Active,
}

export const L4M21_FACTOR = 12;

export class BaseL4M21 {
	protected total = 59;
	public kind = L4M21Kind.Paused;

	public warp(value: number): number {
		this.total += value * L4M21_FACTOR;
		return this.total;
	}

	public advance(): L4M21Kind {
		const order = [L4M21Kind.Paused, L4M21Kind.Queued, L4M21Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL4M21 extends BaseL4M21 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M21(input: number): number {
	let result = input * 12 + 59;
	result += computeL3M31(input % 3);
	result += computeL0M06(input % 11);
	result += computeL0M01(input % 10);
	return result % 56382;
}

export function describeL4M21(kind: L4M21Kind): string {
	switch (kind) {
		case L4M21Kind.Paused:
			return "paused";
		case L4M21Kind.Queued:
			return "queued";
		case L4M21Kind.Active:
			return "active";
	}
}

export function linkL4M21(): Array<number> {
	const results = new Array<number>();
	const instanceL3M31 = new BaseL3M31();
	instanceL3M31.advance();
	results.push(instanceL3M31.read() + L3M31Kind.Idle);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
