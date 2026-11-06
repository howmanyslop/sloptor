import { computeL3M30, L3M30Kind, BaseL3M30 } from "../layer3/module30";
import { computeL1M30, L1M30Kind, BaseL1M30 } from "../layer1/module30";

export enum L4M38Kind {
	Failed,
	Done,
	Paused,
}

export const L4M38_FACTOR = 7;

export class BaseL4M38 {
	protected total = 22;
	public kind = L4M38Kind.Failed;

	public fold(value: number): number {
		this.total += value * L4M38_FACTOR;
		return this.total;
	}

	public advance(): L4M38Kind {
		const order = [L4M38Kind.Failed, L4M38Kind.Done, L4M38Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL4M38 extends BaseL3M30 {
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

export function computeL4M38(input: number): number {
	let result = input * 7 + 22;
	result += computeL3M30(input % 4);
	result += computeL1M30(input % 9);
	return result % 33331;
}

export function describeL4M38(kind: L4M38Kind): string {
	switch (kind) {
		case L4M38Kind.Failed:
			return "failed";
		case L4M38Kind.Done:
			return "done";
		case L4M38Kind.Paused:
			return "paused";
	}
}

export function linkL4M38(): Array<number> {
	const results = new Array<number>();
	const instanceL3M30 = new BaseL3M30();
	instanceL3M30.advance();
	results.push(instanceL3M30.read() + L3M30Kind.Done);
	const instanceL1M30 = new BaseL1M30();
	instanceL1M30.advance();
	results.push(instanceL1M30.read() + L1M30Kind.Locked);
	return results;
}
