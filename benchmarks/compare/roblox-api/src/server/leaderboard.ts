import { Players } from "@rbxts/services";
import { StatName } from "../shared/constants";
import { Signal } from "../shared/signal";

export const statChanged = new Signal<[player: Player, stat: StatName, value: number]>();

function createStat(folder: Folder, name: StatName): IntValue {
	const value = new Instance("IntValue");
	value.Name = name;
	value.Value = 0;
	value.Parent = folder;
	return value;
}

export function setupLeaderstats(player: Player): Folder {
	const folder = new Instance("Folder");
	folder.Name = "leaderstats";
	for (const stat of [StatName.Coins, StatName.Wins, StatName.Deaths]) {
		const value = createStat(folder, stat);
		value.Changed.Connect((newValue) => statChanged.fire(player, stat, newValue));
	}
	folder.Parent = player;
	return folder;
}

function getStat(player: Player, stat: StatName): IntValue | undefined {
	const folder = player.FindFirstChild("leaderstats");
	const value = folder?.FindFirstChild(stat);
	return value && value.IsA("IntValue") ? value : undefined;
}

export function incrementStat(player: Player, stat: StatName, amount = 1): number {
	const value = getStat(player, stat);
	if (!value) return 0;
	value.Value += amount;
	return value.Value;
}

export function readStat(player: Player, stat: StatName): number {
	return getStat(player, stat)?.Value ?? 0;
}

export function resetStats(player: Player): void {
	for (const stat of [StatName.Coins, StatName.Deaths]) {
		const value = getStat(player, stat);
		if (value) value.Value = 0;
	}
}

export function rankPlayers(stat: StatName): Array<[Player, number]> {
	const entries = Players.GetPlayers().map((player): [Player, number] => [player, readStat(player, stat)]);
	entries.sort((left, right) => (left[1] === right[1] ? left[0].Name < right[0].Name : left[1] > right[1]));
	return entries;
}

export function describeStandings(stat: StatName, limit: number): string {
	const lines = new Array<string>();
	rankPlayers(stat).forEach(([player, value], index) => {
		if (index < limit) lines.push(`${index + 1}. ${player.DisplayName} - ${value}`);
	});
	return lines.join("\n");
}

export function watchCharacter(player: Player, onDied: (player: Player) => void): RBXScriptConnection {
	const hook = (character: Model) => {
		const humanoid = character.WaitForChild("Humanoid") as Humanoid;
		humanoid.Died.Connect(() => {
			incrementStat(player, StatName.Deaths);
			onDied(player);
		});
	};
	if (player.Character) hook(player.Character);
	return player.CharacterAdded.Connect(hook);
}
