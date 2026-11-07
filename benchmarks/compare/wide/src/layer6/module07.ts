import { computeL5M15, L5M15Kind, BaseL5M15 } from "../layer5/module15";
import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";
import { computeL2M18, L2M18Kind, BaseL2M18 } from "../layer2/module18";

export enum L6M07Kind {
	Idle,
	Locked,
	Active,
	Done,
}

export const L6M07_FACTOR = 6;

export class BaseL6M07 {
	protected total = 56;
	public kind = L6M07Kind.Idle;

	public scale(value: number): number {
		this.total += value * L6M07_FACTOR;
		return this.total;
	}

	public advance(): L6M07Kind {
		const order = [L6M07Kind.Idle, L6M07Kind.Locked, L6M07Kind.Active, L6M07Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL6M07 extends BaseL6M07 {
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

export function computeL6M07(input: number): number {
	let result = input * 6 + 56;
	result += computeL5M15(input % 4);
	result += computeL0M28(input % 3);
	result += computeL2M18(input % 9);
	return result % 29940;
}

export function describeL6M07(kind: L6M07Kind): string {
	switch (kind) {
		case L6M07Kind.Idle:
			return "idle";
		case L6M07Kind.Locked:
			return "locked";
		case L6M07Kind.Active:
			return "active";
		case L6M07Kind.Done:
			return "done";
	}
}

export function linkL6M07(): Array<number> {
	const results = new Array<number>();
	const instanceL5M15 = new BaseL5M15();
	instanceL5M15.advance();
	results.push(instanceL5M15.read() + L5M15Kind.Locked);
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	const instanceL2M18 = new BaseL2M18();
	instanceL2M18.advance();
	results.push(instanceL2M18.read() + L2M18Kind.Done);
	return results;
}
