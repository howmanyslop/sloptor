import { Lighting, Players, RunService } from "@rbxts/services";
import { buildArena } from "./arena";
import { CoinSpawner } from "./coins";
import { HazardSystem, groundHeightAt, nearbyParts } from "./hazards";
import { setupLeaderstats, statChanged, watchCharacter } from "./leaderboard";
import { DayNightCycle, PlatformAnimator } from "./platforms";
import { RoundController } from "./round";
import { GamePhase } from "../shared/constants";

export function startServer(): void {
	if (!RunService.IsServer()) return;
	const layout = buildArena(1234);
	print(`arena built with ${layout.platforms.size()} platforms and ${layout.spawns.size()} spawns`);
	const height = groundHeightAt(layout.floor.Position, []);
	print("ground height", height, nearbyParts(layout.floor.Position, 30).size());

	const coins = new CoinSpawner(99, 40);
	const hazards = new HazardSystem();
	const platforms = new PlatformAnimator();
	const cycle = new DayNightCycle(Lighting, 4);
	const round = new RoundController(1);

	hazards.start();
	platforms.start();
	cycle.start();

	round.phaseChanged.connect((phase, remaining) => {
		print(`phase ${phase} (${remaining}s)`);
		if (phase === GamePhase.Playing) coins.fill();
		if (phase === GamePhase.Results) print(`collected ${coins.totalCollected()} coins this round`);
	});
	statChanged.connect((player, stat, value) => player.SetAttribute(`Last${stat}`, value));

	const onPlayer = (player: Player) => {
		setupLeaderstats(player);
		watchCharacter(player, (dead) => print(`${dead.Name} died at checkpoint ${hazards.checkpointOf(dead)}`));
	};
	for (const player of Players.GetPlayers()) onPlayer(player);
	Players.PlayerAdded.Connect(onPlayer);

	round.start();
	game.BindToClose(() => {
		round.stop();
		platforms.stop();
		cycle.stop();
		coins.destroy();
	});
}

startServer();
