import { Players, TweenService } from "@rbxts/services";
import { GamePhase, PALETTE } from "../shared/constants";
import { formatNumber, formatTime } from "../shared/math";

function corner(parent: GuiObject, radius: number): UICorner {
	const instance = new Instance("UICorner");
	instance.CornerRadius = new UDim(0, radius);
	instance.Parent = parent;
	return instance;
}

function stroke(parent: GuiObject, color: Color3, thickness: number): UIStroke {
	const instance = new Instance("UIStroke");
	instance.Color = color;
	instance.Thickness = thickness;
	instance.ApplyStrokeMode = Enum.ApplyStrokeMode.Border;
	instance.Parent = parent;
	return instance;
}

function padding(parent: GuiObject, amount: number): UIPadding {
	const instance = new Instance("UIPadding");
	const value = new UDim(0, amount);
	instance.PaddingTop = value;
	instance.PaddingBottom = value;
	instance.PaddingLeft = value;
	instance.PaddingRight = value;
	instance.Parent = parent;
	return instance;
}

function label(parent: Instance, name: string, text: string, size: UDim2, textSize: number): TextLabel {
	const instance = new Instance("TextLabel");
	instance.Name = name;
	instance.Text = text;
	instance.Size = size;
	instance.BackgroundTransparency = 1;
	instance.TextColor3 = PALETTE.text;
	instance.Font = Enum.Font.GothamBold;
	instance.TextSize = textSize;
	instance.TextXAlignment = Enum.TextXAlignment.Left;
	instance.Parent = parent;
	return instance;
}

export class Hud {
	private readonly gui: ScreenGui;
	private readonly panel: Frame;
	private readonly phaseLabel: TextLabel;
	private readonly timerLabel: TextLabel;
	private readonly coinsLabel: TextLabel;
	private readonly progress: Frame;
	private readonly toastHolder: Frame;

	public constructor(player: Player) {
		this.gui = new Instance("ScreenGui");
		this.gui.Name = "Hud";
		this.gui.ResetOnSpawn = false;
		this.gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling;
		this.gui.IgnoreGuiInset = true;

		this.panel = new Instance("Frame");
		this.panel.Name = "Panel";
		this.panel.AnchorPoint = new Vector2(0.5, 0);
		this.panel.Position = new UDim2(0.5, 0, 0, 16);
		this.panel.Size = UDim2.fromOffset(320, 88);
		this.panel.BackgroundColor3 = PALETTE.panel;
		this.panel.BackgroundTransparency = 0.1;
		this.panel.Parent = this.gui;
		corner(this.panel, 12);
		stroke(this.panel, PALETTE.accent, 2);
		padding(this.panel, 10);

		const layout = new Instance("UIListLayout");
		layout.FillDirection = Enum.FillDirection.Vertical;
		layout.SortOrder = Enum.SortOrder.LayoutOrder;
		layout.Padding = new UDim(0, 4);
		layout.Parent = this.panel;

		this.phaseLabel = label(this.panel, "Phase", "Waiting", new UDim2(1, 0, 0, 22), 20);
		this.timerLabel = label(this.panel, "Timer", "0:00", new UDim2(1, 0, 0, 18), 16);
		this.timerLabel.TextColor3 = PALETTE.muted;
		this.coinsLabel = label(this.panel, "Coins", "0 coins", new UDim2(1, 0, 0, 18), 16);

		const track = new Instance("Frame");
		track.Name = "Track";
		track.Size = new UDim2(1, 0, 0, 6);
		track.BackgroundColor3 = PALETTE.background;
		track.LayoutOrder = 10;
		track.Parent = this.panel;
		corner(track, 3);
		this.progress = new Instance("Frame");
		this.progress.Size = UDim2.fromScale(0, 1);
		this.progress.BackgroundColor3 = PALETTE.accent;
		this.progress.Parent = track;
		corner(this.progress, 3);

		this.toastHolder = new Instance("Frame");
		this.toastHolder.Name = "Toasts";
		this.toastHolder.AnchorPoint = new Vector2(1, 1);
		this.toastHolder.Position = new UDim2(1, -16, 1, -16);
		this.toastHolder.Size = UDim2.fromOffset(260, 300);
		this.toastHolder.BackgroundTransparency = 1;
		this.toastHolder.Parent = this.gui;
		const toastLayout = new Instance("UIListLayout");
		toastLayout.VerticalAlignment = Enum.VerticalAlignment.Bottom;
		toastLayout.Padding = new UDim(0, 6);
		toastLayout.Parent = this.toastHolder;

		this.gui.Parent = player.WaitForChild("PlayerGui");
	}

	public setPhase(phase: GamePhase, remaining: number, total: number): void {
		this.phaseLabel.Text = phase;
		this.timerLabel.Text = formatTime(remaining);
		const alpha = total > 0 ? remaining / total : 0;
		TweenService.Create(this.progress, new TweenInfo(0.25), { Size: UDim2.fromScale(alpha, 1) }).Play();
		this.phaseLabel.TextColor3 = phase === GamePhase.Playing ? PALETTE.accent : PALETTE.text;
	}

	public setCoins(coins: number): void {
		this.coinsLabel.Text = `${formatNumber(coins)} coins`;
		const pop = TweenService.Create(this.coinsLabel, new TweenInfo(0.1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out, 0, true), {
			TextSize: 20,
		});
		pop.Play();
	}

	public toast(message: string, color = PALETTE.accent): void {
		const frame = new Instance("TextLabel");
		frame.Size = new UDim2(1, 0, 0, 32);
		frame.BackgroundColor3 = PALETTE.panel;
		frame.TextColor3 = color;
		frame.Text = message;
		frame.Font = Enum.Font.Gotham;
		frame.TextSize = 14;
		frame.BackgroundTransparency = 1;
		frame.TextTransparency = 1;
		frame.Parent = this.toastHolder;
		corner(frame, 8);
		TweenService.Create(frame, new TweenInfo(0.2), { BackgroundTransparency: 0.1, TextTransparency: 0 }).Play();
		task.delay(3, () => {
			const fade = TweenService.Create(frame, new TweenInfo(0.4), { BackgroundTransparency: 1, TextTransparency: 1 });
			fade.Completed.Once(() => frame.Destroy());
			fade.Play();
		});
	}

	public destroy(): void {
		this.gui.Destroy();
	}
}

export function localHud(): Hud | undefined {
	const player = Players.LocalPlayer;
	return player ? new Hud(player) : undefined;
}
