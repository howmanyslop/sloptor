import { Players } from "@rbxts/services";

export const PALETTE = {
	panel: Color3.fromRGB(18, 20, 28),
	panelStroke: Color3.fromRGB(70, 76, 96),
	text: Color3.fromRGB(240, 242, 250),
	muted: Color3.fromRGB(150, 156, 175),
	good: Color3.fromRGB(110, 230, 140),
	bad: Color3.fromRGB(255, 90, 90),
	gold: Color3.fromRGB(255, 210, 80),
} as const;

export interface LabelOptions {
	readonly name: string;
	readonly text?: string;
	readonly size: UDim2;
	readonly position?: UDim2;
	readonly anchor?: Vector2;
	readonly color?: Color3;
	readonly font?: Enum.Font;
	readonly alignX?: Enum.TextXAlignment;
	readonly textSize?: number;
}

export function getPlayerGui(): PlayerGui {
	const player = Players.LocalPlayer;
	return player.WaitForChild("PlayerGui") as PlayerGui;
}

export function createScreen(name: string, displayOrder = 0): ScreenGui {
	const screen = new Instance("ScreenGui");
	screen.Name = name;
	screen.ResetOnSpawn = false;
	screen.IgnoreGuiInset = false;
	screen.DisplayOrder = displayOrder;
	screen.ZIndexBehavior = Enum.ZIndexBehavior.Sibling;
	screen.Parent = getPlayerGui();
	return screen;
}

export function createPanel(parent: Instance, name: string, size: UDim2, position: UDim2, anchor = new Vector2(0, 0)): Frame {
	const frame = new Instance("Frame");
	frame.Name = name;
	frame.Size = size;
	frame.Position = position;
	frame.AnchorPoint = anchor;
	frame.BackgroundColor3 = PALETTE.panel;
	frame.BackgroundTransparency = 0.15;
	frame.BorderSizePixel = 0;

	const corner = new Instance("UICorner");
	corner.CornerRadius = new UDim(0, 10);
	corner.Parent = frame;

	const stroke = new Instance("UIStroke");
	stroke.Color = PALETTE.panelStroke;
	stroke.Thickness = 1;
	stroke.Parent = frame;

	const padding = new Instance("UIPadding");
	for (const side of ["PaddingTop", "PaddingBottom", "PaddingLeft", "PaddingRight"] as const) {
		padding[side] = new UDim(0, 8);
	}
	padding.Parent = frame;

	frame.Parent = parent;
	return frame;
}

export function createLabel(parent: Instance, options: LabelOptions): TextLabel {
	const label = new Instance("TextLabel");
	label.Name = options.name;
	label.Text = options.text ?? "";
	label.Size = options.size;
	label.Position = options.position ?? new UDim2();
	label.AnchorPoint = options.anchor ?? new Vector2();
	label.BackgroundTransparency = 1;
	label.TextColor3 = options.color ?? PALETTE.text;
	label.Font = options.font ?? Enum.Font.GothamMedium;
	label.TextXAlignment = options.alignX ?? Enum.TextXAlignment.Left;
	if (options.textSize !== undefined) {
		label.TextSize = options.textSize;
	} else {
		label.TextScaled = true;
	}
	label.Parent = parent;
	return label;
}

export function createList(parent: Instance, padding = 4): UIListLayout {
	const layout = new Instance("UIListLayout");
	layout.Padding = new UDim(0, padding);
	layout.SortOrder = Enum.SortOrder.LayoutOrder;
	layout.FillDirection = Enum.FillDirection.Vertical;
	layout.Parent = parent;
	return layout;
}
