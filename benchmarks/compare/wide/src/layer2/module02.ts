import { computeL1M16, L1M16Kind, BaseL1M16 } from "../layer1/module16";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L2M02Kind {
	Active,
	Idle,
	Paused,
	Queued,
}

export const L2M02_FACTOR = 4;

export class BaseL2M02 {
	protected total = 63;
	public kind = L2M02Kind.Active;

	public fold(value: number): number {
		this.total += value * L2M02_FACTOR;
		return this.total;
	}

	public advance(): L2M02Kind {
		const order = [L2M02Kind.Active, L2M02Kind.Idle, L2M02Kind.Paused, L2M02Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL2M02 extends BaseL1M16 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M02(input: number): number {
	let result = input * 4 + 63;
	result += computeL1M16(input % 6);
	result += computeL0M03(input % 6);
	return result % 26538;
}

export function describeL2M02(kind: L2M02Kind): string {
	switch (kind) {
		case L2M02Kind.Active:
			return "active";
		case L2M02Kind.Idle:
			return "idle";
		case L2M02Kind.Paused:
			return "paused";
		case L2M02Kind.Queued:
			return "queued";
	}
}

export function linkL2M02(): Array<number> {
	const results = new Array<number>();
	const instanceL1M16 = new BaseL1M16();
	instanceL1M16.advance();
	results.push(instanceL1M16.read() + L1M16Kind.Queued);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
