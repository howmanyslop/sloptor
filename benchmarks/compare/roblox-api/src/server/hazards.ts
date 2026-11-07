import { CollectionService, Players, Workspace } from "@rbxts/services";
import { ATTRIBUTES, CHECKPOINT_TAG, HAZARD_TAG } from "../shared/constants";
import { readNumberAttribute } from "../shared/instance-factory";

const DAMAGE_COOLDOWN = 0.75;

function characterFromPart(part: BasePart): [Player, Humanoid] | undefined {
	const model = part.FindFirstAncestorOfClass("Model");
	if (!model) return undefined;
	const player = Players.GetPlayerFromCharacter(model);
	const humanoid = model.FindFirstChildOfClass("Humanoid");
	return player && humanoid ? [player, humanoid] : undefined;
}

export class HazardSystem {
	private readonly lastHit = new Map<Player, number>();
	private readonly checkpoints = new Map<Player, number>();

	public start(): void {
		for (const hazard of CollectionService.GetTagged(HAZARD_TAG)) this.bindHazard(hazard);
		CollectionService.GetInstanceAddedSignal(HAZARD_TAG).Connect((hazard) => this.bindHazard(hazard));
		for (const checkpoint of CollectionService.GetTagged(CHECKPOINT_TAG)) this.bindCheckpoint(checkpoint);
		Players.PlayerRemoving.Connect((player) => {
			this.lastHit.delete(player);
			this.checkpoints.delete(player);
		});
	}

	private bindHazard(instance: Instance): void {
		if (!instance.IsA("BasePart")) return;
		instance.Touched.Connect((hit) => {
			const found = characterFromPart(hit);
			if (!found) return;
			const [player, humanoid] = found;
			const now = os.clock();
			if (now - (this.lastHit.get(player) ?? 0) < DAMAGE_COOLDOWN) return;
			this.lastHit.set(player, now);
			humanoid.TakeDamage(readNumberAttribute(instance, ATTRIBUTES.damage, 10));
		});
	}

	private bindCheckpoint(instance: Instance): void {
		if (!instance.IsA("BasePart")) return;
		const index = readNumberAttribute(instance, ATTRIBUTES.checkpoint, 0);
		instance.Touched.Connect((hit) => {
			const found = characterFromPart(hit);
			if (!found) return;
			const [player] = found;
			if ((this.checkpoints.get(player) ?? -1) < index) {
				this.checkpoints.set(player, index);
				player.SetAttribute(ATTRIBUTES.checkpoint, index);
			}
		});
	}

	public checkpointOf(player: Player): number {
		return this.checkpoints.get(player) ?? -1;
	}
}

export function castDown(origin: Vector3, ignore: ReadonlyArray<Instance>, distance = 100): RaycastResult | undefined {
	const params = new RaycastParams();
	params.FilterType = Enum.RaycastFilterType.Exclude;
	params.FilterDescendantsInstances = [...ignore];
	params.IgnoreWater = true;
	return Workspace.Raycast(origin, new Vector3(0, -distance, 0), params);
}

export function groundHeightAt(position: Vector3, ignore: ReadonlyArray<Instance>): number | undefined {
	return castDown(position.add(new Vector3(0, 50, 0)), ignore, 200)?.Position.Y;
}

export function lineOfSight(from: Vector3, to: Vector3, ignore: ReadonlyArray<Instance>): boolean {
	const params = new RaycastParams();
	params.FilterType = Enum.RaycastFilterType.Exclude;
	params.FilterDescendantsInstances = [...ignore];
	return Workspace.Raycast(from, to.sub(from), params) === undefined;
}

export function scatterRays(origin: Vector3, count: number, length: number): Array<Vector3> {
	const hits = new Array<Vector3>();
	const params = new RaycastParams();
	for (let index = 0; index < count; index++) {
		const angle = (index / count) * math.pi * 2;
		const direction = new Vector3(math.cos(angle), -0.2, math.sin(angle)).Unit.mul(length);
		const result = Workspace.Raycast(origin, direction, params);
		hits.push(result ? result.Position : origin.add(direction));
	}
	return hits;
}

export function nearbyParts(center: Vector3, radius: number): Array<BasePart> {
	const params = new OverlapParams();
	params.MaxParts = 50;
	return Workspace.GetPartBoundsInRadius(center, radius, params);
}
