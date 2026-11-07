import { CollectionService, Players } from "@rbxts/services";
import type { HazardResponse } from "shared/obstacles/obstacle";
import { RemoteName, fireClient } from "shared/network/remotes";
import { Janitor } from "shared/util/signal";
import type { CheckpointService } from "./checkpoint-service";
import type { ProgressStore } from "./progress-store";
import { Service } from "./service";
import type { SessionService } from "./session-service";
import type { BuiltStage, StageBuilder } from "./stage-builder";

const HAZARD_TAG = "ObbyHazard";
const PUSH_FORCE = 70;
const PUSH_COOLDOWN = 0.4;

function isHazardResponse(value: unknown): value is HazardResponse {
	return value === "kill" || value === "push" || value === "none";
}

export class HazardService extends Service {
	private readonly janitor = new Janitor();
	private readonly pushCooldowns = new Map<Player, number>();
	private readonly dying = new Set<Player>();

	public constructor(
		private readonly builder: StageBuilder,
		private readonly sessions: SessionService,
		private readonly checkpoints: CheckpointService,
		private readonly progress: ProgressStore,
	) {
		super("HazardService");
	}

	public init(): void {}

	public start(): void {
		this.janitor.add(this.builder.stageBuilt.connect((stage) => this.bindStage(stage)));
		this.janitor.add(
			Players.PlayerRemoving.Connect((player) => {
				this.pushCooldowns.delete(player);
				this.dying.delete(player);
			}),
		);
	}

	public shutdown(): void {
		this.janitor.cleanup();
	}

	private bindStage(stage: BuiltStage): void {
		for (const descendant of stage.folder.GetDescendants()) {
			if (descendant.IsA("BasePart") && descendant.GetAttribute("HazardResponse") !== undefined) {
				CollectionService.AddTag(descendant, HAZARD_TAG);
				descendant.Touched.Connect((hit) => this.onTouched(descendant, hit, stage));
			}
		}
	}

	private onTouched(hazard: BasePart, hit: BasePart, stage: BuiltStage): void {
		const character = hit.Parent;
		if (!character || !character.IsA("Model")) {
			return;
		}
		const player = Players.GetPlayerFromCharacter(character);
		const humanoid = character.FindFirstChildOfClass("Humanoid");
		if (!player || !humanoid || humanoid.Health <= 0) {
			return;
		}
		const response = hazard.GetAttribute("HazardResponse");
		if (!isHazardResponse(response)) {
			return;
		}
		switch (response) {
			case "kill":
				this.kill(player, humanoid, `${hazard.Parent?.Name ?? "hazard"}`, stage.spec.index);
				break;
			case "push":
				this.push(player, character, hazard);
				break;
			case "none":
				break;
		}
	}

	private kill(player: Player, humanoid: Humanoid, cause: string, stageIndex: number): void {
		if (this.dying.has(player)) {
			return;
		}
		this.dying.add(player);
		humanoid.Health = 0;
		const session = this.sessions.get(player);
		session?.tracker.recordDeath(cause);
		this.progress.update(player, (data) => ({ ...data, totalDeaths: data.totalDeaths + 1 }));
		fireClient(player, RemoteName.PlayerDied, { cause, stageIndex });
		this.checkpoints.respawn(player);
		task.delay(1, () => this.dying.delete(player));
	}

	private push(player: Player, character: Model, hazard: BasePart): void {
		const now = os.clock();
		if (now - (this.pushCooldowns.get(player) ?? 0) < PUSH_COOLDOWN) {
			return;
		}
		this.pushCooldowns.set(player, now);
		const root = character.FindFirstChild("HumanoidRootPart") as BasePart | undefined;
		if (!root) {
			return;
		}
		const away = root.Position.sub(hazard.Position);
		const flat = new Vector3(away.X, 0, away.Z);
		const direction = flat.Magnitude > 1e-3 ? flat.Unit : new Vector3(0, 0, -1);
		root.AssemblyLinearVelocity = direction.mul(PUSH_FORCE).add(new Vector3(0, PUSH_FORCE * 0.4, 0));
	}
}
