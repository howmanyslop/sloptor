import { computeL5M27, L5M27Kind, BaseL5M27 } from "../layer5/module27";
import { computeL5M22, L5M22Kind, BaseL5M22 } from "../layer5/module22";

export enum L6M03Kind {
	Locked,
	Queued,
	Active,
}

export const L6M03_FACTOR = 4;

export class BaseL6M03 {
	protected total = 6;
	public kind = L6M03Kind.Locked;

	public fold(value: number): number {
		this.total += value * L6M03_FACTOR;
		return this.total;
	}

	public advance(): L6M03Kind {
		const order = [L6M03Kind.Locked, L6M03Kind.Queued, L6M03Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL6M03 extends BaseL6M03 {
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

export function computeL6M03(input: number): number {
	let result = input * 4 + 6;
	result += computeL5M27(input % 5);
	result += computeL5M22(input % 11);
	return result % 34480;
}

export function describeL6M03(kind: L6M03Kind): string {
	switch (kind) {
		case L6M03Kind.Locked:
			return "locked";
		case L6M03Kind.Queued:
			return "queued";
		case L6M03Kind.Active:
			return "active";
	}
}

export function linkL6M03(): Array<number> {
	const results = new Array<number>();
	const instanceL5M27 = new BaseL5M27();
	instanceL5M27.advance();
	results.push(instanceL5M27.read() + L5M27Kind.Paused);
	const instanceL5M22 = new BaseL5M22();
	instanceL5M22.advance();
	results.push(instanceL5M22.read() + L5M22Kind.Queued);
	return results;
}
