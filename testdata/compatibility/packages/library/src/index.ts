export type Receiver<T> = (this: T, value: number) => number;

export const callback: Receiver<void> = (value) => value + 1;

export function direct(this: defined | void, value: number) {
	return value + 1;
}

export const object = {
	value: 10,
	method(this: { value: number }, value: number) {
		return this.value + value;
	},
};
