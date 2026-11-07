import { Players } from "@rbxts/services";
import { RemoteName, onClientEvent } from "shared/network/remotes";
import type { LeaderboardEntry } from "shared/network/remotes";
import { formatDuration, ordinal, padLeft } from "shared/util/format";
import { Controller } from "./controller";
import { PALETTE, createLabel, createList, createPanel, createScreen } from "./ui-factory";

const MEDAL_COLORS: ReadonlyArray<Color3> = [
	Color3.fromRGB(255, 210, 80),
	Color3.fromRGB(200, 205, 215),
	Color3.fromRGB(205, 130, 70),
];

export class LeaderboardPanel extends Controller {
	private body?: Frame;
	private entries: ReadonlyArray<LeaderboardEntry> = [];
	private visible = true;

	public constructor() {
		super("LeaderboardPanel");
	}

	protected onEnable(): void {
		const screen = this.janitor.add(createScreen("ObbyLeaderboard", 4));
		const panel = createPanel(screen, "Panel", UDim2.fromOffset(260, 300), new UDim2(0, 12, 0.3, 0));
		createLabel(panel, {
			name: "Title",
			text: "Today's Fastest",
			size: new UDim2(1, 0, 0, 24),
			font: Enum.Font.GothamBold,
		});
		const body = new Instance("Frame");
		body.Name = "Body";
		body.BackgroundTransparency = 1;
		body.Size = new UDim2(1, 0, 1, -30);
		body.Position = UDim2.fromOffset(0, 30);
		body.Parent = panel;
		createList(body, 2);
		this.body = body;

		this.janitor.add(
			onClientEvent(RemoteName.LeaderboardUpdated, (entries) => {
				this.entries = entries;
				this.render();
			}),
		);
		this.render();
	}

	public setVisible(visible: boolean): void {
		this.visible = visible;
		const panel = this.body?.Parent;
		if (panel && panel.IsA("GuiObject")) {
			panel.Visible = visible;
		}
	}

	public isVisible(): boolean {
		return this.visible;
	}

	private render(): void {
		const body = this.body;
		if (!body) {
			return;
		}
		for (const child of body.GetChildren()) {
			if (child.IsA("TextLabel")) {
				child.Destroy();
			}
		}
		if (this.entries.isEmpty()) {
			createLabel(body, { name: "Empty", text: "No times yet - be the first!", size: new UDim2(1, 0, 0, 20), color: PALETTE.muted, textSize: 14 });
			return;
		}
		const localId = Players.LocalPlayer.UserId;
		this.entries.forEach(({ userId, displayName, seconds }, index) => {
			const place = padLeft(ordinal(index + 1), 4);
			const color = userId === localId ? PALETTE.good : MEDAL_COLORS[index] ?? PALETTE.text;
			const row = createLabel(body, {
				name: `Row${index}`,
				text: `${place}  ${displayName}  ${formatDuration(seconds)}`,
				size: new UDim2(1, 0, 0, 20),
				color,
				textSize: 14,
				font: Enum.Font.Code,
			});
			row.LayoutOrder = index;
		});
	}
}
