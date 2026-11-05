export type Listener<T> = (payload: T) => void;

export class Dispatcher<T> {
	private listeners: Listener<T>[] = [];

	public subscribe(listener: Listener<T>): () => void {
		this.listeners.push(listener);
		return (): void => {
			const remaining: Listener<T>[] = [];
			for (let index = 0; index < this.listeners.length; index += 1) {
				if (this.listeners[index] !== listener) remaining.push(this.listeners[index]);
			}
			this.listeners = remaining;
		};
	}

	public emit(payload: T): void {
		for (let index = 0; index < this.listeners.length; index += 1) {
			this.listeners[index](payload);
		}
	}

	public count(): number {
		return this.listeners.length;
	}
}
