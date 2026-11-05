// oxlint-disable typescript/prefer-for-of -- no
export type Listener<TPayload> = (payload: TPayload) => void;

export class Dispatcher<TPayload> {
	private listeners: Array<Listener<TPayload>> = [];

	public count(): number {
		return this.listeners.length;
	}

	public emit(payload: TPayload): void {
		for (let index = 0; index < this.listeners.length; index += 1) {
			this.listeners[index](payload);
		}
	}

	public subscribe(listener: Listener<TPayload>): () => void {
		this.listeners.push(listener);
		return (): void => {
			const remaining: Array<Listener<TPayload>> = [];
			for (let index = 0; index < this.listeners.length; index += 1) {
				if (this.listeners[index] !== listener) remaining.push(this.listeners[index]);
			}
			this.listeners = remaining;
		};
	}
}
