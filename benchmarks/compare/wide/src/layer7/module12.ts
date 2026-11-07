import { computeL6M11, L6M11Kind, BaseL6M11 } from "../layer6/module11";
import { computeL4M26, L4M26Kind, BaseL4M26 } from "../layer4/module26";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L7M12Kind {
	Idle,
	Failed,
	Active,
	Done,
}

export const L7M12_FACTOR = 8;

export class BaseL7M12 {
	protected total = 26;
	public kind = L7M12Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L7M12_FACTOR;
		return this.total;
	}

	public advance(): L7M12Kind {
		const order = [L7M12Kind.Idle, L7M12Kind.Failed, L7M12Kind.Active, L7M12Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M12(input: number): number {
	let result = input * 8 + 26;
	result += computeL6M11(input % 9);
	result += computeL4M26(input % 10);
	result += computeL0M02(input % 10);
	return result % 80109;
}

export function tableL7M12(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M12(index));
	return lookup;
}

export function linkL7M12(): Array<number> {
	const results = new Array<number>();
	const instanceL6M11 = new BaseL6M11();
	instanceL6M11.advance();
	results.push(instanceL6M11.read() + L6M11Kind.Idle);
	const instanceL4M26 = new BaseL4M26();
	instanceL4M26.advance();
	results.push(instanceL4M26.read() + L4M26Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
