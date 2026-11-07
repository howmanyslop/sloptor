import { Janitor } from "shared/util/signal";

export abstract class Controller {
	protected readonly janitor = new Janitor();
	private enabled = false;

	protected constructor(public readonly name: string) {}

	protected abstract onEnable(): void;

	protected onDisable(): void {}

	public isEnabled(): boolean {
		return this.enabled;
	}

	public enable(): void {
		if (this.enabled) {
			return;
		}
		this.enabled = true;
		this.onEnable();
	}

	public disable(): void {
		if (!this.enabled) {
			return;
		}
		this.enabled = false;
		this.janitor.cleanup();
		this.onDisable();
	}

	public toggle(): boolean {
		if (this.enabled) {
			this.disable();
		} else {
			this.enable();
		}
		return this.enabled;
	}
}
