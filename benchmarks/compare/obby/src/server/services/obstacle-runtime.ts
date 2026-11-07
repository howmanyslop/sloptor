import { Players, RunService } from "@rbxts/services";
import type { Obstacle } from "shared/obstacles/obstacle";
import { Janitor } from "shared/util/signal";
import { Service } from "./service";
import type { StageBuilder } from "./stage-builder";

const ACTIVE_RADIUS = 260;
const CULL_INTERVAL = 0.5;

interface RuntimeStats {
	frames: number;
	activeObstacles: number;
	stepSeconds: number;
}

/**
 * Steps obstacle animations on Heartbeat. Obstacles far from every player are
 * paused so large courses stay cheap.
 */
export class ObstacleRuntime extends Service {
	private readonly janitor = new Janitor();
	private readonly stats: RuntimeStats = { frames: 0, activeObstacles: 0, stepSeconds: 0 };
	private cullTimer = 0;
	private paused = false;

	public constructor(private readonly builder: StageBuilder) {
		super("ObstacleRuntime");
	}

	public init(): void {}

	public start(): void {
		this.janitor.add(RunService.Heartbeat.Connect((deltaTime) => this.onHeartbeat(deltaTime)));
		this.janitor.add(this.builder.courseBuilt.connect(() => this.cull()));
	}

	public setPaused(paused: boolean): void {
		this.paused = paused;
	}

	public getStats(): Readonly<RuntimeStats> {
		return { ...this.stats };
	}

	public shutdown(): void {
		this.janitor.cleanup();
	}

	private onHeartbeat(deltaTime: number): void {
		if (this.paused) {
			return;
		}
		const started = os.clock();
		this.cullTimer += deltaTime;
		if (this.cullTimer >= CULL_INTERVAL) {
			this.cullTimer = 0;
			this.cull();
		}
		this.builder.getRegistry().stepAll(deltaTime);
		this.stats.frames += 1;
		this.stats.stepSeconds = os.clock() - started;
	}

	private cull(): void {
		const positions = this.playerPositions();
		let active = 0;
		for (const obstacle of this.builder.getRegistry().all()) {
			const shouldRun = positions.isEmpty() ? false : this.isNearAny(obstacle, positions);
			obstacle.setActive(shouldRun);
			if (shouldRun) {
				active += 1;
			}
		}
		this.stats.activeObstacles = active;
	}

	private isNearAny(obstacle: Obstacle, positions: ReadonlyArray<Vector3>): boolean {
		const pivot = obstacle.getModel()?.GetPivot().Position;
		if (pivot === undefined) {
			return false;
		}
		return positions.some((position) => position.sub(pivot).Magnitude <= ACTIVE_RADIUS);
	}

	private playerPositions(): Array<Vector3> {
		return Players.GetPlayers().mapFiltered((player) => {
			const root = player.Character?.FindFirstChild("HumanoidRootPart") as BasePart | undefined;
			return root?.Position;
		});
	}
}
