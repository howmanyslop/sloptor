import { computeL4M31, L4M31Kind, BaseL4M31 } from "../layer4/module31";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L5M38Kind {
	Paused,
	Locked,
	Queued,
	Done,
	Active,
}

export const L5M38_FACTOR = 10;

export class BaseL5M38 {
	protected total = 44;
	public kind = L5M38Kind.Paused;

	public blend(value: number): number {
		this.total += value * L5M38_FACTOR;
		return this.total;
	}

	public advance(): L5M38Kind {
		const order = [L5M38Kind.Paused, L5M38Kind.Locked, L5M38Kind.Queued, L5M38Kind.Done, L5M38Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL5M38 extends BaseL5M38 {
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

export function computeL5M38(input: number): number {
	let result = input * 10 + 44;
	result += computeL4M31(input % 6);
	result += computeL0M03(input % 11);
	return result % 4979;
}

export function describeL5M38(kind: L5M38Kind): string {
	switch (kind) {
		case L5M38Kind.Paused:
			return "paused";
		case L5M38Kind.Locked:
			return "locked";
		case L5M38Kind.Queued:
			return "queued";
		case L5M38Kind.Done:
			return "done";
		case L5M38Kind.Active:
			return "active";
	}
}

export function linkL5M38(): Array<number> {
	const results = new Array<number>();
	const instanceL4M31 = new BaseL4M31();
	instanceL4M31.advance();
	results.push(instanceL4M31.read() + L4M31Kind.Failed);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
