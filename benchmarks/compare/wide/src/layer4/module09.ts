import { computeL3M27, L3M27Kind, BaseL3M27 } from "../layer3/module27";
import { computeL0M26, L0M26Kind, BaseL0M26 } from "../layer0/module26";
import { computeL0M14, L0M14Kind, BaseL0M14 } from "../layer0/module14";

export enum L4M09Kind {
	Idle,
	Active,
	Paused,
}

export const L4M09_FACTOR = 19;

export class BaseL4M09 {
	protected total = 92;
	public kind = L4M09Kind.Idle;

	public shift(value: number): number {
		this.total += value * L4M09_FACTOR;
		return this.total;
	}

	public advance(): L4M09Kind {
		const order = [L4M09Kind.Idle, L4M09Kind.Active, L4M09Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL4M09 extends BaseL4M09 {
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

export function computeL4M09(input: number): number {
	let result = input * 19 + 92;
	result += computeL3M27(input % 3);
	result += computeL0M26(input % 10);
	result += computeL0M14(input % 9);
	return result % 8908;
}

export function describeL4M09(kind: L4M09Kind): string {
	switch (kind) {
		case L4M09Kind.Idle:
			return "idle";
		case L4M09Kind.Active:
			return "active";
		case L4M09Kind.Paused:
			return "paused";
	}
}

export function linkL4M09(): Array<number> {
	const results = new Array<number>();
	const instanceL3M27 = new BaseL3M27();
	instanceL3M27.advance();
	results.push(instanceL3M27.read() + L3M27Kind.Queued);
	const instanceL0M26 = new BaseL0M26();
	instanceL0M26.advance();
	results.push(instanceL0M26.read() + L0M26Kind.Idle);
	const instanceL0M14 = new BaseL0M14();
	instanceL0M14.advance();
	results.push(instanceL0M14.read() + L0M14Kind.Idle);
	return results;
}
