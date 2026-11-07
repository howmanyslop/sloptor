import { CollectionService, Players, TweenService, Workspace } from "@rbxts/services";
import { ARENA_ORIGIN, ARENA_SIZE, ATTRIBUTES, COIN_TAG, StatName } from "../shared/constants";
import { addParticles, getOrCreateFolder, readNumberAttribute } from "../shared/instance-factory";
import { randomPointInBox } from "../shared/math";
import { Janitor } from "../shared/signal";
import { incrementStat } from "./leaderboard";

const spinInfo = new TweenInfo(2, Enum.EasingStyle.Linear, Enum.EasingDirection.InOut, -1);
const bobInfo = new TweenInfo(1.2, Enum.EasingStyle.Sine, Enum.EasingDirection.InOut, -1, true);
const collectInfo = new TweenInfo(0.35, Enum.EasingStyle.Back, Enum.EasingDirection.In);

export class CoinSpawner {
	private readonly folder = getOrCreateFolder(Workspace, "Coins");
	private readonly random: Random;
	private readonly janitor = new Janitor();
	private readonly perCoin = new Map<Instance, Janitor>();
	private collected = 0;

	public constructor(seed: number, private readonly maximum: number) {
		this.random = new Random(seed);
		this.janitor.connect(CollectionService.GetInstanceAddedSignal(COIN_TAG), (instance) => this.track(instance));
		this.janitor.connect(CollectionService.GetInstanceRemovedSignal(COIN_TAG), (instance) => this.untrack(instance));
	}

	public spawnCoin(): Part {
		const coin = new Instance("Part");
		coin.Name = "Coin";
		coin.Shape = Enum.PartType.Cylinder;
		coin.Size = new Vector3(0.4, 3, 3);
		coin.Color = Color3.fromRGB(255, 205, 50);
		coin.Material = Enum.Material.Neon;
		coin.Anchored = true;
		coin.CanCollide = false;
		const position = randomPointInBox(this.random, ARENA_ORIGIN.add(new Vector3(0, 4, 0)), ARENA_SIZE.mul(new Vector3(0.9, 0, 0.9)));
		coin.CFrame = new CFrame(position).mul(CFrame.Angles(0, 0, math.rad(90)));
		const value = this.random.NextNumber() < 0.1 ? 10 : 1;
		coin.SetAttribute(ATTRIBUTES.value, value);
		if (value > 1) addParticles(coin, coin.Color);
		coin.Parent = this.folder;
		CollectionService.AddTag(coin, COIN_TAG);
		return coin;
	}

	public fill(): number {
		let spawned = 0;
		while (CollectionService.GetTagged(COIN_TAG).size() < this.maximum) {
			this.spawnCoin();
			spawned++;
		}
		return spawned;
	}

	public totalCollected(): number {
		return this.collected;
	}

	private track(instance: Instance): void {
		if (!instance.IsA("BasePart")) return;
		const janitor = new Janitor();
		this.perCoin.set(instance, janitor);
		const base = instance.CFrame;
		janitor.add(TweenService.Create(instance, bobInfo, { CFrame: base.add(new Vector3(0, 1.5, 0)) }));
		const spin = TweenService.Create(instance, spinInfo, { Orientation: instance.Orientation.add(new Vector3(0, 360, 0)) });
		janitor.add(spin);
		spin.Play();
		janitor.connect(instance.Touched, (hit) => this.onTouched(instance, hit));
	}

	private untrack(instance: Instance): void {
		this.perCoin.get(instance)?.cleanup();
		this.perCoin.delete(instance);
	}

	private onTouched(coin: BasePart, hit: BasePart): void {
		const character = hit.FindFirstAncestorOfClass("Model");
		const player = character ? Players.GetPlayerFromCharacter(character) : undefined;
		if (!player || coin.GetAttribute("Collected") === true) return;
		coin.SetAttribute("Collected", true);
		const value = readNumberAttribute(coin, ATTRIBUTES.value, 1);
		incrementStat(player, StatName.Coins, value);
		this.collected += value;
		const tween = TweenService.Create(coin, collectInfo, { Size: new Vector3(0.1, 0.1, 0.1), Transparency: 1 });
		tween.Completed.Connect(() => {
			CollectionService.RemoveTag(coin, COIN_TAG);
			coin.Destroy();
		});
		tween.Play();
	}

	public destroy(): void {
		for (const [, janitor] of this.perCoin) janitor.cleanup();
		this.perCoin.clear();
		this.janitor.cleanup();
		this.folder.ClearAllChildren();
	}
}
