export type Listener<T extends ReadonlyArray<unknown>> = (...args: T) => void;

export interface Connection {
	readonly connected: boolean;
	disconnect(): void;
}

export class Signal<T extends ReadonlyArray<unknown> = []> {
	private readonly listeners = new Map<number, Listener<T>>();
	private nextId = 0;

	public connect(listener: Listener<T>): Connection {
		const id = this.nextId++;
		this.listeners.set(id, listener);
		const listeners = this.listeners;
		const connection = {
			connected: true,
			disconnect() {
				connection.connected = false;
				listeners.delete(id);
			},
		};
		return connection;
	}

	public once(listener: Listener<T>): Connection {
		const connection = this.connect((...args) => {
			connection.disconnect();
			listener(...args);
		});
		return connection;
	}

	public fire(...args: T): void {
		for (const [, listener] of this.listeners) task.spawn(listener, ...args);
	}

	public listenerCount(): number {
		return this.listeners.size();
	}

	public destroy(): void {
		this.listeners.clear();
	}
}

type Cleanup = RBXScriptConnection | Instance | Connection | (() => void);

export class Janitor {
	private readonly tasks = new Array<Cleanup>();

	public add<T extends Cleanup>(item: T): T {
		this.tasks.push(item);
		return item;
	}

	public connect<T extends Callback>(signal: RBXScriptSignal<T>, callback: T): RBXScriptConnection {
		return this.add(signal.Connect(callback));
	}

	public cleanup(): void {
		for (let index = this.tasks.size() - 1; index >= 0; index--) {
			const item = this.tasks[index];
			if (typeIs(item, "function")) item();
			else if (typeIs(item, "RBXScriptConnection")) item.Disconnect();
			else if (typeIs(item, "Instance")) item.Destroy();
			else item.disconnect();
		}
		this.tasks.clear();
	}

	public size(): number {
		return this.tasks.size();
	}
}
