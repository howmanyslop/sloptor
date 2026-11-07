import { computeL1M02, L1M02Kind, BaseL1M02 } from "../layer1/module02";

export enum L2M00Kind {
	Idle,
	Active,
	Failed,
	Paused,
}

export const L2M00_FACTOR = 4;

export class BaseL2M00 {
	protected total = 93;
	public kind = L2M00Kind.Idle;

	public tally(value: number): number {
		this.total += value * L2M00_FACTOR;
		return this.total;
	}

	public advance(): L2M00Kind {
		const order = [L2M00Kind.Idle, L2M00Kind.Active, L2M00Kind.Failed, L2M00Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL2M00 extends BaseL1M02 {
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

export function computeL2M00(input: number): number {
	let result = input * 4 + 93;
	result += computeL1M02(input % 9);
	return result % 40385;
}

export function describeL2M00(kind: L2M00Kind): string {
	switch (kind) {
		case L2M00Kind.Idle:
			return "idle";
		case L2M00Kind.Active:
			return "active";
		case L2M00Kind.Failed:
			return "failed";
		case L2M00Kind.Paused:
			return "paused";
	}
}

export function linkL2M00(): Array<number> {
	const results = new Array<number>();
	const instanceL1M02 = new BaseL1M02();
	instanceL1M02.advance();
	results.push(instanceL1M02.read() + L1M02Kind.Locked);
	return results;
}
