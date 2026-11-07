import { computeL1M33, L1M33Kind, BaseL1M33 } from "../layer1/module33";
import { computeL1M02, L1M02Kind, BaseL1M02 } from "../layer1/module02";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M33Kind {
	Locked,
	Failed,
	Done,
	Queued,
	Idle,
}

export const L2M33_FACTOR = 5;

export class BaseL2M33 {
	protected total = 47;
	public kind = L2M33Kind.Locked;

	public weigh(value: number): number {
		this.total += value * L2M33_FACTOR;
		return this.total;
	}

	public advance(): L2M33Kind {
		const order = [L2M33Kind.Locked, L2M33Kind.Failed, L2M33Kind.Done, L2M33Kind.Queued, L2M33Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL2M33 extends BaseL1M33 {
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

export function computeL2M33(input: number): number {
	let result = input * 5 + 47;
	result += computeL1M33(input % 4);
	result += computeL1M02(input % 4);
	result += computeL0M02(input % 4);
	return result % 22743;
}

export function describeL2M33(kind: L2M33Kind): string {
	switch (kind) {
		case L2M33Kind.Locked:
			return "locked";
		case L2M33Kind.Failed:
			return "failed";
		case L2M33Kind.Done:
			return "done";
		case L2M33Kind.Queued:
			return "queued";
		case L2M33Kind.Idle:
			return "idle";
	}
}

export function linkL2M33(): Array<number> {
	const results = new Array<number>();
	const instanceL1M33 = new BaseL1M33();
	instanceL1M33.advance();
	results.push(instanceL1M33.read() + L1M33Kind.Idle);
	const instanceL1M02 = new BaseL1M02();
	instanceL1M02.advance();
	results.push(instanceL1M02.read() + L1M02Kind.Locked);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
