import { Signal } from "../util/signal";

export type RunEvent =
	| { readonly type: "started"; readonly at: number }
	| { readonly type: "checkpoint"; readonly stageIndex: number; readonly at: number }
	| { readonly type: "death"; readonly stageIndex: number; readonly at: number; readonly cause: string }
	| { readonly type: "finished"; readonly at: number }
	| { readonly type: "abandoned"; readonly at: number };

export interface StageSplit {
	readonly stageIndex: number;
	readonly seconds: number;
	readonly deaths: number;
}

export const enum RunStatus {
	Idle,
	Running,
	Finished,
	Abandoned,
}

export class RunTracker {
	public readonly splitRecorded = new Signal<[split: StageSplit]>();
	public readonly statusChanged = new Signal<[status: RunStatus]>();

	private status = RunStatus.Idle;
	private readonly events = new Array<RunEvent>();
	private readonly splits = new Array<StageSplit>();
	private stageStartedAt = 0;
	private startedAt = 0;
	private deathsThisStage = 0;
	private currentStage = 0;

	public constructor(
		public readonly userId: number,
		private readonly clock: () => number = os.clock,
	) {}

	public getStatus(): RunStatus {
		return this.status;
	}

	public getCurrentStage(): number {
		return this.currentStage;
	}

	public getSplits(): ReadonlyArray<StageSplit> {
		return this.splits;
	}

	public getElapsed(): number {
		return this.status === RunStatus.Running ? this.clock() - this.startedAt : this.totalSeconds();
	}

	public totalSeconds(): number {
		return this.splits.reduce((total, split) => total + split.seconds, 0);
	}

	public totalDeaths(): number {
		return this.events.filter((event) => event.type === "death").size();
	}

	public start(stageIndex = 0): void {
		const now = this.clock();
		this.startedAt = now;
		this.stageStartedAt = now;
		this.currentStage = stageIndex;
		this.deathsThisStage = 0;
		this.splits.clear();
		this.push({ type: "started", at: now });
		this.setStatus(RunStatus.Running);
	}

	public reachCheckpoint(stageIndex: number): StageSplit | undefined {
		if (this.status !== RunStatus.Running || stageIndex !== this.currentStage) {
			return undefined;
		}
		const now = this.clock();
		const split: StageSplit = { stageIndex, seconds: now - this.stageStartedAt, deaths: this.deathsThisStage };
		this.splits.push(split);
		this.push({ type: "checkpoint", stageIndex, at: now });
		this.splitRecorded.fire(split);
		this.currentStage = stageIndex + 1;
		this.stageStartedAt = now;
		this.deathsThisStage = 0;
		return split;
	}

	public recordDeath(cause: string): void {
		if (this.status !== RunStatus.Running) {
			return;
		}
		this.deathsThisStage += 1;
		this.push({ type: "death", stageIndex: this.currentStage, at: this.clock(), cause });
	}

	public finish(): void {
		if (this.status === RunStatus.Running) {
			this.push({ type: "finished", at: this.clock() });
			this.setStatus(RunStatus.Finished);
		}
	}

	public abandon(): void {
		if (this.status === RunStatus.Running) {
			this.push({ type: "abandoned", at: this.clock() });
			this.setStatus(RunStatus.Abandoned);
		}
	}

	public deathCauses(): Map<string, number> {
		const causes = new Map<string, number>();
		for (const event of this.events) {
			if (event.type === "death") {
				causes.set(event.cause, (causes.get(event.cause) ?? 0) + 1);
			}
		}
		return causes;
	}

	public describeEvent(event: RunEvent): string {
		const at = string.format("%.2f", event.at - this.startedAt);
		switch (event.type) {
			case "started":
				return `[${at}] run started`;
			case "checkpoint":
				return `[${at}] reached stage ${event.stageIndex + 1}`;
			case "death":
				return `[${at}] died on stage ${event.stageIndex + 1} (${event.cause})`;
			case "finished":
				return `[${at}] finished`;
			case "abandoned":
				return `[${at}] abandoned`;
		}
	}

	public destroy(): void {
		this.splitRecorded.destroy();
		this.statusChanged.destroy();
	}

	private push(event: RunEvent): void {
		this.events.push(event);
		if (this.events.size() > 512) {
			this.events.shift();
		}
	}

	private setStatus(status: RunStatus): void {
		if (this.status !== status) {
			this.status = status;
			this.statusChanged.fire(status);
		}
	}
}
