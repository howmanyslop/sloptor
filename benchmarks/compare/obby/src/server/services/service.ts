export abstract class Service {
	private static readonly registry = new Array<Service>();
	private started = false;

	protected constructor(public readonly name: string) {
		Service.registry.push(this);
	}

	public static getAll(): ReadonlyArray<Service> {
		return Service.registry;
	}

	/** Called for every service before any `start`; must not yield. */
	public abstract init(): void;

	/** Called after all services have initialised; may connect events and yield. */
	public abstract start(): void;

	public shutdown(): void {}

	public isStarted(): boolean {
		return this.started;
	}

	public static bootAll(): void {
		const order = [...Service.registry].sort((a, b) => a.getPriority() > b.getPriority());
		for (const service of order) {
			const started = os.clock();
			service.init();
			print(`[boot] init ${service.name} (${string.format("%.1f", (os.clock() - started) * 1000)}ms)`);
		}
		for (const service of order) {
			task.spawn(() => {
				service.start();
				service.started = true;
			});
		}
	}

	public static shutdownAll(): void {
		for (const service of [...Service.registry].sort((a, b) => a.getPriority() < b.getPriority())) {
			const [success, message] = pcall(() => service.shutdown());
			if (!success) {
				warn(`[shutdown] ${service.name} failed: ${message}`);
			}
		}
	}

	/** Higher priorities initialise first. */
	protected getPriority(): number {
		return 0;
	}
}
