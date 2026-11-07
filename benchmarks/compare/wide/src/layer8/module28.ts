import { computeL7M04, L7M04Kind, BaseL7M04 } from "../layer7/module04";
import { computeL4M06, L4M06Kind, BaseL4M06 } from "../layer4/module06";
import { computeL2M28, L2M28Kind, BaseL2M28 } from "../layer2/module28";

export enum L8M28Kind {
	Done,
	Paused,
	Active,
	Idle,
}

export const L8M28_FACTOR = 14;

export class BaseL8M28 {
	protected total = 35;
	public kind = L8M28Kind.Done;

	public fold(value: number): number {
		this.total += value * L8M28_FACTOR;
		return this.total;
	}

	public advance(): L8M28Kind {
		const order = [L8M28Kind.Done, L8M28Kind.Paused, L8M28Kind.Active, L8M28Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M28 extends BaseL7M04 {
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

export function computeL8M28(input: number): number {
	let result = input * 14 + 35;
	result += computeL7M04(input % 10);
	result += computeL4M06(input % 4);
	result += computeL2M28(input % 8);
	return result % 45461;
}

export const L8M28_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 35,
	chain: (value) => computeL8M28(value),
};

export function linkL8M28(): Array<number> {
	const results = new Array<number>();
	const instanceL7M04 = new BaseL7M04();
	instanceL7M04.advance();
	results.push(instanceL7M04.read() + L7M04Kind.Done);
	const instanceL4M06 = new BaseL4M06();
	instanceL4M06.advance();
	results.push(instanceL4M06.read() + L4M06Kind.Paused);
	const instanceL2M28 = new BaseL2M28();
	instanceL2M28.advance();
	results.push(instanceL2M28.read() + L2M28Kind.Failed);
	return results;
}
