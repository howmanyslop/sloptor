import { computeL8M24, L8M24Kind, BaseL8M24 } from "../layer8/module24";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L9M25Kind {
	Active,
	Queued,
	Done,
}

export const L9M25_FACTOR = 7;

export class BaseL9M25 {
	protected total = 83;
	public kind = L9M25Kind.Active;

	public fold(value: number): number {
		this.total += value * L9M25_FACTOR;
		return this.total;
	}

	public advance(): L9M25Kind {
		const order = [L9M25Kind.Active, L9M25Kind.Queued, L9M25Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL9M25 extends BaseL8M24 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M25(input: number): number {
	let result = input * 7 + 83;
	result += computeL8M24(input % 6);
	result += computeL0M02(input % 3);
	return result % 90398;
}

export function describeL9M25(kind: L9M25Kind): string {
	switch (kind) {
		case L9M25Kind.Active:
			return "active";
		case L9M25Kind.Queued:
			return "queued";
		case L9M25Kind.Done:
			return "done";
	}
}

export function linkL9M25(): Array<number> {
	const results = new Array<number>();
	const instanceL8M24 = new BaseL8M24();
	instanceL8M24.advance();
	results.push(instanceL8M24.read() + L8M24Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
