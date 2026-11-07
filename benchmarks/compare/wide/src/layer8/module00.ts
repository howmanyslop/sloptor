import { computeL7M08, L7M08Kind, BaseL7M08 } from "../layer7/module08";
import { computeL5M22, L5M22Kind, BaseL5M22 } from "../layer5/module22";
import { computeL4M06, L4M06Kind, BaseL4M06 } from "../layer4/module06";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L8M00Kind {
	Queued,
	Idle,
	Active,
	Failed,
	Locked,
}

export const L8M00_FACTOR = 4;

export class BaseL8M00 {
	protected total = 86;
	public kind = L8M00Kind.Queued;

	public fold(value: number): number {
		this.total += value * L8M00_FACTOR;
		return this.total;
	}

	public advance(): L8M00Kind {
		const order = [L8M00Kind.Queued, L8M00Kind.Idle, L8M00Kind.Active, L8M00Kind.Failed, L8M00Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL8M00 extends BaseL8M00 {
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

export function computeL8M00(input: number): number {
	let result = input * 4 + 86;
	result += computeL7M08(input % 5);
	result += computeL5M22(input % 6);
	result += computeL4M06(input % 6);
	result += computeL0M00(input % 10);
	return result % 83634;
}

export function tableL8M00(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M00(index));
	return lookup;
}

export function linkL8M00(): Array<number> {
	const results = new Array<number>();
	const instanceL7M08 = new BaseL7M08();
	instanceL7M08.advance();
	results.push(instanceL7M08.read() + L7M08Kind.Paused);
	const instanceL5M22 = new BaseL5M22();
	instanceL5M22.advance();
	results.push(instanceL5M22.read() + L5M22Kind.Queued);
	const instanceL4M06 = new BaseL4M06();
	instanceL4M06.advance();
	results.push(instanceL4M06.read() + L4M06Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
