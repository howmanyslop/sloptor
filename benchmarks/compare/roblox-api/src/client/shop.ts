import { TweenService } from "@rbxts/services";
import { PALETTE } from "../shared/constants";
import { make } from "../shared/instance-factory";
import { formatNumber, hueShift } from "../shared/math";
import { Signal } from "../shared/signal";

export interface ShopItem {
	readonly id: string;
	readonly title: string;
	readonly price: number;
	readonly color: Color3;
}

export const CATALOG: ReadonlyArray<ShopItem> = [
	{ id: "speed", title: "Speed Boots", price: 50, color: Color3.fromRGB(80, 200, 255) },
	{ id: "jump", title: "Spring Shoes", price: 75, color: Color3.fromRGB(120, 255, 120) },
	{ id: "magnet", title: "Coin Magnet", price: 150, color: Color3.fromRGB(255, 200, 60) },
	{ id: "shield", title: "Hazard Shield", price: 200, color: Color3.fromRGB(200, 120, 255) },
	{ id: "trail", title: "Rainbow Trail", price: 500, color: Color3.fromRGB(255, 100, 160) },
	{ id: "pet", title: "Tiny Robot", price: 1000, color: Color3.fromRGB(180, 180, 200) },
];

export class ShopWindow {
	public readonly purchased = new Signal<[item: ShopItem]>();
	private readonly root: Frame;
	private readonly buttons = new Map<string, TextButton>();
	private open = false;

	public constructor(parent: ScreenGui, private balance: number) {
		this.root = make("Frame", {
			Name: "Shop",
			AnchorPoint: new Vector2(0.5, 0.5),
			Position: UDim2.fromScale(0.5, 1.5),
			Size: UDim2.fromOffset(480, 360),
			BackgroundColor3: PALETTE.background,
			Parent: parent,
		});
		make("UICorner", { CornerRadius: new UDim(0, 16), Parent: this.root });
		make("TextLabel", {
			Name: "Title",
			Text: "Shop",
			Size: new UDim2(1, 0, 0, 48),
			BackgroundTransparency: 1,
			TextColor3: PALETTE.text,
			Font: Enum.Font.GothamBlack,
			TextSize: 28,
			Parent: this.root,
		});
		const grid = make("ScrollingFrame", {
			Name: "Grid",
			Position: UDim2.fromOffset(16, 56),
			Size: new UDim2(1, -32, 1, -72),
			BackgroundTransparency: 1,
			ScrollBarThickness: 6,
			AutomaticCanvasSize: Enum.AutomaticSize.Y,
			CanvasSize: new UDim2(),
			Parent: this.root,
		});
		make("UIGridLayout", { CellSize: UDim2.fromOffset(140, 120), CellPadding: UDim2.fromOffset(8, 8), Parent: grid });
		for (const item of CATALOG) this.buttons.set(item.id, this.createCard(grid, item));
		this.refresh();
	}

	private createCard(parent: Instance, item: ShopItem): TextButton {
		const button = make("TextButton", {
			Name: item.id,
			Text: "",
			BackgroundColor3: PALETTE.panel,
			AutoButtonColor: false,
			Parent: parent,
		});
		make("UICorner", { CornerRadius: new UDim(0, 10), Parent: button });
		make("UIGradient", {
			Color: new ColorSequence(item.color, hueShift(item.color, 0.08)),
			Rotation: 45,
			Parent: make("Frame", { Size: new UDim2(1, -16, 0, 50), Position: UDim2.fromOffset(8, 8), Parent: button }),
		});
		make("TextLabel", {
			Text: item.title,
			Position: UDim2.fromOffset(8, 64),
			Size: new UDim2(1, -16, 0, 20),
			BackgroundTransparency: 1,
			TextColor3: PALETTE.text,
			Font: Enum.Font.GothamBold,
			TextSize: 14,
			Parent: button,
		});
		make("TextLabel", {
			Name: "Price",
			Text: `${formatNumber(item.price)} coins`,
			Position: UDim2.fromOffset(8, 88),
			Size: new UDim2(1, -16, 0, 18),
			BackgroundTransparency: 1,
			TextColor3: PALETTE.muted,
			Font: Enum.Font.Gotham,
			TextSize: 12,
			Parent: button,
		});
		button.MouseEnter.Connect(() => TweenService.Create(button, new TweenInfo(0.15), { BackgroundColor3: PALETTE.panel.Lerp(item.color, 0.25) }).Play());
		button.MouseLeave.Connect(() => TweenService.Create(button, new TweenInfo(0.15), { BackgroundColor3: PALETTE.panel }).Play());
		button.Activated.Connect(() => this.tryPurchase(item));
		return button;
	}

	private tryPurchase(item: ShopItem): void {
		if (this.balance < item.price) {
			const button = this.buttons.get(item.id)!;
			const original = button.Position;
			const shake = TweenService.Create(button, new TweenInfo(0.05, Enum.EasingStyle.Linear, Enum.EasingDirection.InOut, 3, true), {
				Position: original.add(UDim2.fromOffset(4, 0)),
			});
			shake.Play();
			return;
		}
		this.balance -= item.price;
		this.purchased.fire(item);
		this.refresh();
	}

	public setBalance(balance: number): void {
		this.balance = balance;
		this.refresh();
	}

	private refresh(): void {
		for (const item of CATALOG) {
			const price = this.buttons.get(item.id)?.FindFirstChild("Price");
			if (price && price.IsA("TextLabel")) price.TextColor3 = this.balance >= item.price ? PALETTE.accent : PALETTE.muted;
		}
	}

	public toggle(): void {
		this.open = !this.open;
		const target = this.open ? UDim2.fromScale(0.5, 0.5) : UDim2.fromScale(0.5, 1.5);
		TweenService.Create(this.root, new TweenInfo(0.35, Enum.EasingStyle.Quint), { Position: target }).Play();
	}
}
