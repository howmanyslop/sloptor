import { UserInputService } from "@rbxts/services";
import { Signal } from "shared/util/signal";
import { Controller } from "./controller";

export type InputAction = "respawn" | "restart" | "restartDaily" | "toggleTimer" | "toggleLeaderboard" | "toggleMotion";

interface Binding {
	readonly action: InputAction;
	readonly key: Enum.KeyCode;
	readonly requiresShift: boolean;
	readonly description: string;
}

const DEFAULT_BINDINGS: ReadonlyArray<Binding> = [
	{ action: "respawn", key: Enum.KeyCode.R, requiresShift: false, description: "Respawn at checkpoint" },
	{ action: "restart", key: Enum.KeyCode.R, requiresShift: true, description: "Restart custom run" },
	{ action: "restartDaily", key: Enum.KeyCode.D, requiresShift: true, description: "Restart daily run" },
	{ action: "toggleTimer", key: Enum.KeyCode.T, requiresShift: false, description: "Toggle timer" },
	{ action: "toggleLeaderboard", key: Enum.KeyCode.Tab, requiresShift: false, description: "Toggle leaderboard" },
	{ action: "toggleMotion", key: Enum.KeyCode.M, requiresShift: false, description: "Toggle reduced motion" },
];

export class InputController extends Controller {
	public readonly actionTriggered = new Signal<[action: InputAction]>();
	private readonly bindings: Array<Binding>;
	private readonly cooldowns = new Map<InputAction, number>();

	public constructor(bindings: ReadonlyArray<Binding> = DEFAULT_BINDINGS, private readonly cooldown = 0.3) {
		super("InputController");
		this.bindings = [...bindings].sort((a, b) => a.requiresShift && !b.requiresShift);
	}

	protected onEnable(): void {
		this.janitor.add(
			UserInputService.InputBegan.Connect((input, processed) => {
				if (processed || input.UserInputType !== Enum.UserInputType.Keyboard) {
					return;
				}
				const action = this.resolve(input.KeyCode);
				if (action !== undefined) {
					this.trigger(action);
				}
			}),
		);
	}

	public describeBindings(): Array<string> {
		return this.bindings.map(({ key, requiresShift, description }) => `${requiresShift ? "Shift+" : ""}${key.Name}: ${description}`);
	}

	private resolve(key: Enum.KeyCode): InputAction | undefined {
		const shiftHeld =
			UserInputService.IsKeyDown(Enum.KeyCode.LeftShift) || UserInputService.IsKeyDown(Enum.KeyCode.RightShift);
		return this.bindings.find((binding) => binding.key === key && binding.requiresShift === shiftHeld)?.action;
	}

	private trigger(action: InputAction): void {
		const now = os.clock();
		if (now - (this.cooldowns.get(action) ?? 0) < this.cooldown) {
			return;
		}
		this.cooldowns.set(action, now);
		this.actionTriggered.fire(action);
	}
}
