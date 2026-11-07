export type SignalListener<T extends Array<unknown>> = (...args: T) => void;

export interface SignalConnection {
	readonly isConnected: () => boolean;
	readonly disconnect: () => void;
}

interface ListenerNode<T extends Array<unknown>> {
	listener: SignalListener<T>;
	connected: boolean;
	once: boolean;
}

export class Signal<T extends Array<unknown> = []> {
	private nodes = new Array<ListenerNode<T>>();
	private firing = 0;
	private destroyed = false;

	public connect(listener: SignalListener<T>): SignalConnection {
		return this.attach(listener, false);
	}

	public once(listener: SignalListener<T>): SignalConnection {
		return this.attach(listener, true);
	}

	public fire(...args: T): void {
		if (this.destroyed) {
			return;
		}
		this.firing += 1;
		for (const node of [...this.nodes]) {
			if (!node.connected) {
				continue;
			}
			if (node.once) {
				node.connected = false;
			}
			node.listener(...args);
		}
		this.firing -= 1;
		if (this.firing === 0) {
			this.compact();
		}
	}

	public wait(): T {
		const thread = coroutine.running();
		this.once((...args) => {
			task.spawn(thread, ...args);
		});
		return coroutine.yield() as unknown as T;
	}

	public getListenerCount(): number {
		return this.nodes.filter((node) => node.connected).size();
	}

	public disconnectAll(): void {
		for (const node of this.nodes) {
			node.connected = false;
		}
		this.nodes.clear();
	}

	public destroy(): void {
		this.disconnectAll();
		this.destroyed = true;
	}

	private attach(listener: SignalListener<T>, once: boolean): SignalConnection {
		const node: ListenerNode<T> = { listener, connected: true, once };
		this.nodes.push(node);
		return {
			isConnected: () => node.connected,
			disconnect: () => {
				node.connected = false;
				if (this.firing === 0) {
					this.compact();
				}
			},
		};
	}

	private compact(): void {
		this.nodes = this.nodes.filter((node) => node.connected);
	}
}

export class Janitor {
	private readonly tasks = new Array<() => void>();

	public add<T extends Instance | SignalConnection | RBXScriptConnection | (() => void)>(item: T): T {
		if (typeIs(item, "function")) {
			this.tasks.push(item);
		} else if (typeIs(item, "Instance")) {
			this.tasks.push(() => item.Destroy());
		} else if (typeIs(item, "RBXScriptConnection")) {
			this.tasks.push(() => item.Disconnect());
		} else {
			this.tasks.push(() => item.disconnect());
		}
		return item;
	}

	public cleanup(): void {
		while (!this.tasks.isEmpty()) {
			const job = this.tasks.pop();
			job?.();
		}
	}
}
