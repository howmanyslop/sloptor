export type Listener<T> = (payload: T) => void;

export class Dispatcher<T> {
	private listeners: Listener<T>[] = [];

	public subscribe(listener: Listener<T>): () => void {
		this.listeners.push(listener);
		return (): void => {
			const remaining: Listener<T>[] = [];
			for (let i = 0; i < this.listeners.length; i++) {
				if (this.listeners[i] !== listener) remaining.push(this.listeners[i]);
			}
			this.listeners = remaining;
		};
	}

	public emit(payload: T): void {
		for (let i = 0; i < this.listeners.length; i++) {
			this.listeners[i](payload);
		}
	}

	public count(): number {
		return this.listeners.length;
	}
}
