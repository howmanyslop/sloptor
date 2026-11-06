import { computeL5M08, L5M08Kind, BaseL5M08 } from "../layer5/module08";
import { computeL1M02, L1M02Kind, BaseL1M02 } from "../layer1/module02";
import { computeL1M24, L1M24Kind, BaseL1M24 } from "../layer1/module24";

export enum L6M06Kind {
	Done,
	Queued,
	Idle,
}

export const L6M06_FACTOR = 9;

export class BaseL6M06 {
	protected total = 48;
	public kind = L6M06Kind.Done;

	public mix(value: number): number {
		this.total += value * L6M06_FACTOR;
		return this.total;
	}

	public advance(): L6M06Kind {
		const order = [L6M06Kind.Done, L6M06Kind.Queued, L6M06Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL6M06 extends BaseL6M06 {
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

export function computeL6M06(input: number): number {
	let result = input * 9 + 48;
	result += computeL5M08(input % 10);
	result += computeL1M02(input % 10);
	result += computeL1M24(input % 8);
	return result % 96507;
}

export function tableL6M06(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M06(index));
	return lookup;
}

export function linkL6M06(): Array<number> {
	const results = new Array<number>();
	const instanceL5M08 = new BaseL5M08();
	instanceL5M08.advance();
	results.push(instanceL5M08.read() + L5M08Kind.Idle);
	const instanceL1M02 = new BaseL1M02();
	instanceL1M02.advance();
	results.push(instanceL1M02.read() + L1M02Kind.Locked);
	const instanceL1M24 = new BaseL1M24();
	instanceL1M24.advance();
	results.push(instanceL1M24.read() + L1M24Kind.Active);
	return results;
}
