import { TweenService } from "@rbxts/services";
import { TIER_SETTINGS } from "shared/config";
import { RemoteName, onClientEvent } from "shared/network/remotes";
import type { CourseAnnouncement } from "shared/network/remotes";
import type { DifficultyTier } from "shared/types";
import { formatDuration, ordinal } from "shared/util/format";
import { Controller } from "./controller";
import { PALETTE, createLabel, createList, createPanel, createScreen } from "./ui-factory";
import type { Split, TimerController } from "./timer-controller";

const MAX_SPLIT_ROWS = 5;

export class HudController extends Controller {
	private screen?: ScreenGui;
	private stageLabel?: TextLabel;
	private timerLabel?: TextLabel;
	private courseLabel?: TextLabel;
	private splitList?: Frame;
	private bannerLabel?: TextLabel;
	private tiers: ReadonlyArray<DifficultyTier> = [];
	private stageCount = 0;
	private timerVisible = true;

	public constructor(private readonly timer: TimerController) {
		super("HudController");
	}

	protected onEnable(): void {
		const screen = this.janitor.add(createScreen("ObbyHud", 5));
		this.screen = screen;

		const top = createPanel(screen, "TopBar", UDim2.fromOffset(320, 84), new UDim2(0.5, 0, 0, 12), new Vector2(0.5, 0));
		this.courseLabel = createLabel(top, { name: "Course", size: new UDim2(1, 0, 0, 18), color: PALETTE.muted });
		this.stageLabel = createLabel(top, {
			name: "Stage",
			size: new UDim2(0.5, 0, 0, 40),
			position: UDim2.fromOffset(0, 22),
			font: Enum.Font.GothamBold,
		});
		this.timerLabel = createLabel(top, {
			name: "Timer",
			size: new UDim2(0.5, 0, 0, 40),
			position: new UDim2(0.5, 0, 0, 22),
			font: Enum.Font.Code,
			alignX: Enum.TextXAlignment.Right,
			text: formatDuration(0),
		});

		const splits = createPanel(screen, "Splits", UDim2.fromOffset(220, 160), new UDim2(1, -12, 0.3, 0), new Vector2(1, 0));
		createList(splits, 2);
		this.splitList = splits;

		this.bannerLabel = createLabel(screen, {
			name: "Banner",
			size: UDim2.fromScale(0.6, 0.08),
			position: UDim2.fromScale(0.5, 0.35),
			anchor: new Vector2(0.5, 0.5),
			alignX: Enum.TextXAlignment.Center,
			font: Enum.Font.GothamBlack,
		});
		this.bannerLabel.TextTransparency = 1;

		this.janitor.add(this.timer.ticked.connect((elapsed) => this.updateTimer(elapsed)));
		this.janitor.add(this.timer.splitAdded.connect((split) => this.addSplitRow(split)));
		this.janitor.add(onClientEvent(RemoteName.CourseAnnounced, (announcement) => this.onCourse(announcement)));
		this.janitor.add(
			onClientEvent(RemoteName.RunFinished, ({ totalSeconds, deaths, rank, newBest }) => {
				const rankText = rank !== undefined ? ` - ${ordinal(rank)} today` : "";
				this.showBanner(`Finished in ${formatDuration(totalSeconds)} (${deaths} deaths)${rankText}`, newBest ? PALETTE.gold : PALETTE.text);
			}),
		);
		this.janitor.add(
			onClientEvent(RemoteName.PlayerDied, ({ cause }) => this.showBanner(`Eliminated by ${cause}`, PALETTE.bad, 1.2)),
		);
	}

	protected onDisable(): void {
		this.screen = undefined;
	}

	public setTimerVisible(visible: boolean): void {
		this.timerVisible = visible;
		if (this.timerLabel) {
			this.timerLabel.Visible = visible;
		}
	}

	public setStage(stageIndex: number): void {
		const label = this.stageLabel;
		if (!label) {
			return;
		}
		const tier = this.tiers[stageIndex];
		label.Text = `Stage ${math.min(stageIndex + 1, this.stageCount)}/${this.stageCount}`;
		label.TextColor3 = tier !== undefined ? TIER_SETTINGS[tier].color : PALETTE.text;
	}

	public showBanner(text: string, color: Color3, holdSeconds = 2.5): void {
		const banner = this.bannerLabel;
		if (!banner) {
			return;
		}
		banner.Text = text;
		banner.TextColor3 = color;
		const fadeIn = TweenService.Create(banner, new TweenInfo(0.25), { TextTransparency: 0 });
		const fadeOut = TweenService.Create(banner, new TweenInfo(0.6, Enum.EasingStyle.Quad, Enum.EasingDirection.In, 0, false, holdSeconds), {
			TextTransparency: 1,
		});
		fadeIn.Completed.Once(() => fadeOut.Play());
		fadeIn.Play();
	}

	private onCourse({ label, stageCount, tiers, themeName, secondsUntilReset }: CourseAnnouncement): void {
		this.tiers = tiers;
		this.stageCount = stageCount;
		if (this.courseLabel) {
			const reset = secondsUntilReset > 0 ? ` - resets in ${formatDuration(secondsUntilReset)}` : "";
			this.courseLabel.Text = `${label}${reset}`;
		}
		this.splitList?.GetChildren().forEach((child) => {
			if (child.IsA("TextLabel")) {
				child.Destroy();
			}
		});
		this.setStage(0);
		this.showBanner(themeName, PALETTE.gold);
	}

	private updateTimer(elapsed: number): void {
		if (this.timerLabel && this.timerVisible) {
			this.timerLabel.Text = formatDuration(elapsed);
		}
	}

	private addSplitRow({ stageIndex, seconds, delta }: Split): void {
		const list = this.splitList;
		if (!list) {
			return;
		}
		const rows = list.GetChildren().filter((child): child is TextLabel => child.IsA("TextLabel"));
		if (rows.size() >= MAX_SPLIT_ROWS) {
			const oldest = rows.sort((a, b) => a.LayoutOrder < b.LayoutOrder)[0];
			oldest?.Destroy();
		}
		const deltaText = delta === undefined ? "" : string.format(" (%s%.2f)", delta <= 0 ? "-" : "+", math.abs(delta));
		const row = createLabel(list, {
			name: `Split${stageIndex}`,
			size: new UDim2(1, 0, 0, 22),
			text: `${stageIndex + 1}. ${formatDuration(seconds)}${deltaText}`,
			color: delta === undefined ? PALETTE.text : delta <= 0 ? PALETTE.good : PALETTE.bad,
			textSize: 16,
			font: Enum.Font.Code,
		});
		row.LayoutOrder = stageIndex;
		this.setStage(stageIndex + 1);
	}
}
