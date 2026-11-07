import { CollectionService, Workspace } from "@rbxts/services";
import { ARENA_ORIGIN, ARENA_SIZE, CHECKPOINT_TAG, HAZARD_TAG, PALETTE, PLATFORM_TAG, TEAM_COLORS } from "../shared/constants";
import { addLight, getOrCreateFolder, makePart, setAttributes } from "../shared/instance-factory";
import { pointsOnCircle } from "../shared/math";

export interface ArenaLayout {
	readonly floor: Part;
	readonly walls: Array<Part>;
	readonly spawns: Array<SpawnLocation>;
	readonly platforms: Array<Part>;
	readonly checkpoints: Array<Part>;
}

function buildWalls(folder: Folder): Array<Part> {
	const walls = new Array<Part>();
	const height = 30;
	const thickness = 2;
	const halfX = ARENA_SIZE.X / 2;
	const halfZ = ARENA_SIZE.Z / 2;
	const specs: Array<[Vector3, Vector3]> = [
		[new Vector3(ARENA_SIZE.X, height, thickness), new Vector3(0, height / 2, halfZ)],
		[new Vector3(ARENA_SIZE.X, height, thickness), new Vector3(0, height / 2, -halfZ)],
		[new Vector3(thickness, height, ARENA_SIZE.Z), new Vector3(halfX, height / 2, 0)],
		[new Vector3(thickness, height, ARENA_SIZE.Z), new Vector3(-halfX, height / 2, 0)],
	];
	specs.forEach(([size, offset], index) => {
		const wall = makePart(`Wall${index}`, size, ARENA_ORIGIN.add(offset), PALETTE.panel, folder);
		wall.Transparency = 0.4;
		wall.Material = Enum.Material.ForceField;
		walls.push(wall);
	});
	return walls;
}

function buildSpawns(folder: Folder): Array<SpawnLocation> {
	const spawns = new Array<SpawnLocation>();
	const positions = pointsOnCircle(ARENA_ORIGIN.Position.add(new Vector3(0, 1, 0)), 70, TEAM_COLORS.size());
	positions.forEach((position, index) => {
		const spawn = new Instance("SpawnLocation");
		spawn.Name = `Spawn${index}`;
		spawn.Size = new Vector3(8, 1, 8);
		spawn.CFrame = CFrame.lookAt(position, ARENA_ORIGIN.Position);
		spawn.Anchored = true;
		spawn.Neutral = false;
		spawn.TeamColor = new BrickColor(TEAM_COLORS[index]);
		spawn.Color = TEAM_COLORS[index];
		spawn.Duration = 3;
		spawn.Parent = folder;
		spawns.push(spawn);
	});
	return spawns;
}

function buildPlatforms(folder: Folder, random: Random): Array<Part> {
	const platforms = new Array<Part>();
	for (let index = 0; index < 8; index++) {
		const angle = (index / 8) * math.pi * 2;
		const position = ARENA_ORIGIN.Position.add(new Vector3(math.cos(angle) * 40, 6 + index * 2, math.sin(angle) * 40));
		const platform = makePart(`Platform${index}`, new Vector3(10, 1, 10), new CFrame(position), PALETTE.accent, folder);
		setAttributes(platform, { Speed: random.NextNumber(0.5, 2), Amplitude: random.NextInteger(4, 12), Phase: angle });
		CollectionService.AddTag(platform, PLATFORM_TAG);
		platforms.push(platform);
	}
	return platforms;
}

function buildHazards(folder: Folder, random: Random): void {
	for (let index = 0; index < 12; index++) {
		const position = new Vector3(random.NextNumber(-80, 80), ARENA_ORIGIN.Y + 1, random.NextNumber(-80, 80));
		const hazard = makePart(`Hazard${index}`, new Vector3(6, 0.4, 6), new CFrame(position), PALETTE.danger, folder);
		hazard.Material = Enum.Material.Neon;
		hazard.CanCollide = false;
		hazard.SetAttribute("Damage", random.NextInteger(10, 35));
		addLight(hazard, PALETTE.danger, 10);
		CollectionService.AddTag(hazard, HAZARD_TAG);
	}
}

function buildCheckpoints(folder: Folder): Array<Part> {
	const checkpoints = new Array<Part>();
	for (let index = 0; index < 5; index++) {
		const position = ARENA_ORIGIN.Position.add(new Vector3(-80 + index * 40, 2, -60));
		const checkpoint = makePart(`Checkpoint${index}`, new Vector3(4, 4, 4), new CFrame(position), Color3.fromHSV(index / 5, 0.7, 1), folder);
		checkpoint.Shape = Enum.PartType.Ball;
		checkpoint.CanCollide = false;
		checkpoint.SetAttribute("Checkpoint", index);
		CollectionService.AddTag(checkpoint, CHECKPOINT_TAG);
		checkpoints.push(checkpoint);
	}
	return checkpoints;
}

export function buildArena(seed: number): ArenaLayout {
	const random = new Random(seed);
	const root = getOrCreateFolder(Workspace, "Arena");
	root.ClearAllChildren();
	const floor = makePart("Floor", ARENA_SIZE, ARENA_ORIGIN, PALETTE.background, root);
	floor.Material = Enum.Material.Slate;
	const walls = buildWalls(getOrCreateFolder(root, "Walls"));
	const spawns = buildSpawns(getOrCreateFolder(root, "Spawns"));
	const platforms = buildPlatforms(getOrCreateFolder(root, "Platforms"), random);
	buildHazards(getOrCreateFolder(root, "Hazards"), random);
	const checkpoints = buildCheckpoints(getOrCreateFolder(root, "Checkpoints"));
	return { floor, walls, spawns, platforms, checkpoints };
}
