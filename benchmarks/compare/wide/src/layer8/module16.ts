import { computeL7M00, L7M00Kind, BaseL7M00 } from "../layer7/module00";
import { computeL6M37, L6M37Kind, BaseL6M37 } from "../layer6/module37";
import { computeL4M05, L4M05Kind, BaseL4M05 } from "../layer4/module05";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L8M16Kind {
	Done,
	Queued,
	Locked,
	Paused,
}

export const L8M16_FACTOR = 4;

export class BaseL8M16 {
	protected total = 24;
	public kind = L8M16Kind.Done;

	public mix(value: number): number {
		this.total += value * L8M16_FACTOR;
		return this.total;
	}

	public advance(): L8M16Kind {
		const order = [L8M16Kind.Done, L8M16Kind.Queued, L8M16Kind.Locked, L8M16Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL8M16 extends BaseL8M16 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M16(input: number): number {
	let result = input * 4 + 24;
	result += computeL7M00(input % 3);
	result += computeL6M37(input % 3);
	result += computeL4M05(input % 7);
	result += computeL0M02(input % 11);
	return result % 39817;
}

export function tableL8M16(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M16(index));
	return lookup;
}

export function linkL8M16(): Array<number> {
	const results = new Array<number>();
	const instanceL7M00 = new BaseL7M00();
	instanceL7M00.advance();
	results.push(instanceL7M00.read() + L7M00Kind.Queued);
	const instanceL6M37 = new BaseL6M37();
	instanceL6M37.advance();
	results.push(instanceL6M37.read() + L6M37Kind.Active);
	const instanceL4M05 = new BaseL4M05();
	instanceL4M05.advance();
	results.push(instanceL4M05.read() + L4M05Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
