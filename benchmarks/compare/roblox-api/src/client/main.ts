import { Players, ReplicatedStorage, RunService } from "@rbxts/services";
import { GamePhase, INTERMISSION_LENGTH, PALETTE, ROUND_LENGTH } from "../shared/constants";
import { Janitor } from "../shared/signal";
import { InputMap, OrbitCamera, screenToWorld } from "./camera";
import { billboard, burst, flashScreen, highlight, setAtmosphere } from "./effects";
import { Hud } from "./hud";
import { ShopWindow } from "./shop";

function totalFor(phase: GamePhase): number {
	if (phase === GamePhase.Playing) return ROUND_LENGTH;
	if (phase === GamePhase.Intermission) return INTERMISSION_LENGTH;
	return 8;
}

export function startClient(): Janitor {
	const janitor = new Janitor();
	if (!RunService.IsClient()) return janitor;
	const player = Players.LocalPlayer;
	const hud = new Hud(player);
	janitor.add(() => hud.destroy());
	const shopGui = new Instance("ScreenGui");
	shopGui.Name = "ShopGui";
	shopGui.Parent = player.WaitForChild("PlayerGui");
	janitor.add(shopGui);
	const shop = new ShopWindow(shopGui, 0);
	janitor.add(shop.purchased.connect((item) => hud.toast(`Bought ${item.title}!`, item.color)));

	const state = ReplicatedStorage.WaitForChild("RoundState");
	const refresh = () => {
		const phase = (state.GetAttribute("Phase") as GamePhase | undefined) ?? GamePhase.Waiting;
		const remaining = (state.GetAttribute("Remaining") as number | undefined) ?? 0;
		hud.setPhase(phase, remaining, totalFor(phase));
	};
	janitor.connect(state.GetAttributeChangedSignal("Remaining"), refresh);
	janitor.connect(state.GetAttributeChangedSignal("Phase"), () => {
		refresh();
		const phase = state.GetAttribute("Phase");
		if (phase === GamePhase.Results) {
			hud.toast(`Winner: ${tostring(state.GetAttribute("Winner"))}`);
			flashScreen(PALETTE.accent);
		}
	});

	const leaderstats = player.WaitForChild("leaderstats");
	const coins = leaderstats.WaitForChild("Coins") as IntValue;
	janitor.connect(coins.Changed, (value) => {
		hud.setCoins(value);
		shop.setBalance(value);
		const root = player.Character?.FindFirstChild("HumanoidRootPart");
		if (root && root.IsA("BasePart")) {
			burst(root.Position, PALETTE.accent, 8);
			billboard(root, "+coin", PALETTE.accent);
		}
	});

	const camera = new OrbitCamera(() => {
		const root = player.Character?.FindFirstChild("HumanoidRootPart");
		return root && root.IsA("BasePart") ? root.Position : undefined;
	});
	const input = new InputMap();
	let orbiting = false;
	janitor.add(input.bind(Enum.KeyCode.B, () => shop.toggle()));
	janitor.add(
		input.bind(Enum.KeyCode.C, () => {
			orbiting = !orbiting;
			if (orbiting) camera.enable();
			else camera.disable();
		}),
	);
	janitor.add(
		input.bind(Enum.KeyCode.F, () => {
			const hit = screenToWorld(new Vector2(400, 300), 500);
			if (hit) highlight(hit.Instance, PALETTE.accent, 2);
		}),
	);
	setAtmosphere(0.3, PALETTE.muted);
	refresh();
	return janitor;
}

startClient();
