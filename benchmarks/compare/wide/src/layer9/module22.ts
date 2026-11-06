import { computeL8M03, L8M03Kind, BaseL8M03 } from "../layer8/module03";

export enum L9M22Kind {
	Locked,
	Done,
	Queued,
	Paused,
}

export const L9M22_FACTOR = 17;

export class BaseL9M22 {
	protected total = 92;
	public kind = L9M22Kind.Locked;

	public weigh(value: number): number {
		this.total += value * L9M22_FACTOR;
		return this.total;
	}

	public advance(): L9M22Kind {
		const order = [L9M22Kind.Locked, L9M22Kind.Done, L9M22Kind.Queued, L9M22Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL9M22 extends BaseL8M03 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M22(input: number): number {
	let result = input * 17 + 92;
	result += computeL8M03(input % 4);
	return result % 58706;
}

export function describeL9M22(kind: L9M22Kind): string {
	switch (kind) {
		case L9M22Kind.Locked:
			return "locked";
		case L9M22Kind.Done:
			return "done";
		case L9M22Kind.Queued:
			return "queued";
		case L9M22Kind.Paused:
			return "paused";
	}
}

export function linkL9M22(): Array<number> {
	const results = new Array<number>();
	const instanceL8M03 = new BaseL8M03();
	instanceL8M03.advance();
	results.push(instanceL8M03.read() + L8M03Kind.Queued);
	return results;
}
