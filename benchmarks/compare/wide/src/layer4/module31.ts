import { computeL3M21, L3M21Kind, BaseL3M21 } from "../layer3/module21";
import { computeL3M05, L3M05Kind, BaseL3M05 } from "../layer3/module05";

export enum L4M31Kind {
	Failed,
	Idle,
	Active,
}

export const L4M31_FACTOR = 19;

export class BaseL4M31 {
	protected total = 29;
	public kind = L4M31Kind.Failed;

	public tally(value: number): number {
		this.total += value * L4M31_FACTOR;
		return this.total;
	}

	public advance(): L4M31Kind {
		const order = [L4M31Kind.Failed, L4M31Kind.Idle, L4M31Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL4M31 extends BaseL3M21 {
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

export function computeL4M31(input: number): number {
	let result = input * 19 + 29;
	result += computeL3M21(input % 4);
	result += computeL3M05(input % 11);
	return result % 9215;
}

export function describeL4M31(kind: L4M31Kind): string {
	switch (kind) {
		case L4M31Kind.Failed:
			return "failed";
		case L4M31Kind.Idle:
			return "idle";
		case L4M31Kind.Active:
			return "active";
	}
}

export function linkL4M31(): Array<number> {
	const results = new Array<number>();
	const instanceL3M21 = new BaseL3M21();
	instanceL3M21.advance();
	results.push(instanceL3M21.read() + L3M21Kind.Done);
	const instanceL3M05 = new BaseL3M05();
	instanceL3M05.advance();
	results.push(instanceL3M05.read() + L3M05Kind.Failed);
	return results;
}
