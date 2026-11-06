import { RunService } from "@rbxts/services";
import { Signal } from "shared/util/signal";
import { Controller } from "./controller";

export interface Split {
	readonly stageIndex: number;
	readonly seconds: number;
	readonly delta?: number;
}

/**
 * Client-side run clock. The server is authoritative for recorded times; this
 * only drives the HUD so it never waits on network latency.
 */
export class TimerController extends Controller {
	public readonly ticked = new Signal<[elapsed: number]>();
	public readonly splitAdded = new Signal<[split: Split]>();

	private startedAt?: number;
	private stoppedAt?: number;
	private readonly splits = new Array<Split>();
	private accumulator = 0;

	public constructor(private readonly tickRate = 1 / 20) {
		super("TimerController");
	}

	protected onEnable(): void {
		this.janitor.add(
			RunService.RenderStepped.Connect((deltaTime) => {
				this.accumulator += deltaTime;
				if (this.accumulator >= this.tickRate) {
					this.accumulator = 0;
					this.ticked.fire(this.getElapsed());
				}
			}),
		);
	}

	public reset(): void {
		this.startedAt = os.clock();
		this.stoppedAt = undefined;
		this.splits.clear();
	}

	public stop(): void {
		if (this.startedAt !== undefined && this.stoppedAt === undefined) {
			this.stoppedAt = os.clock();
		}
	}

	public isRunning(): boolean {
		return this.startedAt !== undefined && this.stoppedAt === undefined;
	}

	public getElapsed(): number {
		if (this.startedAt === undefined) {
			return 0;
		}
		return (this.stoppedAt ?? os.clock()) - this.startedAt;
	}

	public addSplit(stageIndex: number, seconds: number, personalBest?: number): Split {
		const split: Split = { stageIndex, seconds, delta: personalBest !== undefined ? seconds - personalBest : undefined };
		this.splits.push(split);
		this.splitAdded.fire(split);
		return split;
	}

	public getSplits(): ReadonlyArray<Split> {
		return this.splits;
	}

	public bestSplit(): Split | undefined {
		return this.splits.reduce<Split | undefined>(
			(best, split) => (best === undefined || split.seconds < best.seconds ? split : best),
			undefined,
		);
	}
}
