import { RemoteName, fireServer, onClientEvent } from "shared/network/remotes";
import { CheckpointEffects } from "./controllers/checkpoint-effects";
import type { Controller } from "./controllers/controller";
import { HudController } from "./controllers/hud-controller";
import { InputController } from "./controllers/input-controller";
import type { InputAction } from "./controllers/input-controller";
import { LeaderboardPanel } from "./controllers/leaderboard-panel";
import { PALETTE } from "./controllers/ui-factory";
import { TimerController } from "./controllers/timer-controller";

const timer = new TimerController();
const hud = new HudController(timer);
const effects = new CheckpointEffects();
const input = new InputController();
const leaderboard = new LeaderboardPanel();

const controllers: ReadonlyArray<Controller> = [timer, hud, effects, input, leaderboard];
for (const controller of controllers) {
	controller.enable();
}

const preferences = { showTimer: true, reducedMotion: false };

function handleAction(action: InputAction): void {
	switch (action) {
		case "respawn":
			fireServer(RemoteName.RequestRespawn, undefined);
			break;
		case "restart":
		case "restartDaily":
			fireServer(RemoteName.RequestRestart, { daily: action === "restartDaily" });
			timer.reset();
			hud.setStage(0);
			break;
		case "toggleTimer":
			preferences.showTimer = !preferences.showTimer;
			hud.setTimerVisible(preferences.showTimer);
			fireServer(RemoteName.UpdateSettings, { key: "showTimer", value: preferences.showTimer });
			break;
		case "toggleLeaderboard":
			leaderboard.setVisible(!leaderboard.isVisible());
			break;
		case "toggleMotion":
			preferences.reducedMotion = !preferences.reducedMotion;
			effects.setReducedMotion(preferences.reducedMotion);
			hud.showBanner(`Reduced motion ${preferences.reducedMotion ? "on" : "off"}`, PALETTE.muted, 1);
			fireServer(RemoteName.UpdateSettings, { key: "reducedMotion", value: preferences.reducedMotion });
			break;
	}
}

input.actionTriggered.connect(handleAction);

onClientEvent(RemoteName.CourseAnnounced, () => timer.reset());
onClientEvent(RemoteName.StageReached, ({ stageIndex, splitSeconds, personalBest }) => {
	timer.addSplit(stageIndex, splitSeconds, personalBest);
});
onClientEvent(RemoteName.RunFinished, () => timer.stop());

print(`[obby] client ready; ${controllers.filter((controller) => controller.isEnabled()).size()} controllers enabled`);
for (const line of input.describeBindings()) {
	print(`  ${line}`);
}
