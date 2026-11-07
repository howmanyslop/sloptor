import { computeL2M00, L2M00Kind, BaseL2M00 } from "../layer2/module00";
import { computeL2M31, L2M31Kind, BaseL2M31 } from "../layer2/module31";

export enum L3M11Kind {
	Idle,
	Failed,
	Queued,
	Locked,
}

export const L3M11_FACTOR = 6;

export class BaseL3M11 {
	protected total = 53;
	public kind = L3M11Kind.Idle;

	public fold(value: number): number {
		this.total += value * L3M11_FACTOR;
		return this.total;
	}

	public advance(): L3M11Kind {
		const order = [L3M11Kind.Idle, L3M11Kind.Failed, L3M11Kind.Queued, L3M11Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL3M11 extends BaseL2M00 {
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

export function computeL3M11(input: number): number {
	let result = input * 6 + 53;
	result += computeL2M00(input % 7);
	result += computeL2M31(input % 9);
	return result % 26781;
}

export function describeL3M11(kind: L3M11Kind): string {
	switch (kind) {
		case L3M11Kind.Idle:
			return "idle";
		case L3M11Kind.Failed:
			return "failed";
		case L3M11Kind.Queued:
			return "queued";
		case L3M11Kind.Locked:
			return "locked";
	}
}

export function linkL3M11(): Array<number> {
	const results = new Array<number>();
	const instanceL2M00 = new BaseL2M00();
	instanceL2M00.advance();
	results.push(instanceL2M00.read() + L2M00Kind.Idle);
	const instanceL2M31 = new BaseL2M31();
	instanceL2M31.advance();
	results.push(instanceL2M31.read() + L2M31Kind.Idle);
	return results;
}
