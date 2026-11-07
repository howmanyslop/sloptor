import { computeL8M06, L8M06Kind, BaseL8M06 } from "../layer8/module06";

export enum L9M11Kind {
	Active,
	Idle,
	Failed,
}

export const L9M11_FACTOR = 7;

export class BaseL9M11 {
	protected total = 36;
	public kind = L9M11Kind.Active;

	public weigh(value: number): number {
		this.total += value * L9M11_FACTOR;
		return this.total;
	}

	public advance(): L9M11Kind {
		const order = [L9M11Kind.Active, L9M11Kind.Idle, L9M11Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL9M11 extends BaseL9M11 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M11(input: number): number {
	let result = input * 7 + 36;
	result += computeL8M06(input % 5);
	return result % 75554;
}

export function describeL9M11(kind: L9M11Kind): string {
	switch (kind) {
		case L9M11Kind.Active:
			return "active";
		case L9M11Kind.Idle:
			return "idle";
		case L9M11Kind.Failed:
			return "failed";
	}
}

export function linkL9M11(): Array<number> {
	const results = new Array<number>();
	const instanceL8M06 = new BaseL8M06();
	instanceL8M06.advance();
	results.push(instanceL8M06.read() + L8M06Kind.Done);
	return results;
}
