import { Debris, Lighting, TweenService, Workspace } from "@rbxts/services";
import { PALETTE } from "../shared/constants";
import { bezier, colorRamp, pointsOnCircle } from "../shared/math";

export function burst(position: Vector3, color: Color3, count: number): void {
	const folder = new Instance("Folder");
	folder.Name = "Burst";
	folder.Parent = Workspace;
	for (const point of pointsOnCircle(position, 1, count)) {
		const shard = new Instance("Part");
		shard.Size = new Vector3(0.4, 0.4, 0.4);
		shard.CFrame = new CFrame(position);
		shard.Color = color;
		shard.Material = Enum.Material.Neon;
		shard.Anchored = true;
		shard.CanCollide = false;
		shard.Parent = folder;
		const target = position.add(point.sub(position).mul(6)).add(new Vector3(0, 3, 0));
		TweenService.Create(shard, new TweenInfo(0.6, Enum.EasingStyle.Quad, Enum.EasingDirection.Out), {
			CFrame: new CFrame(target),
			Transparency: 1,
			Size: new Vector3(0.05, 0.05, 0.05),
		}).Play();
	}
	Debris.AddItem(folder, 1);
}

export function arcProjectile(from: Vector3, to: Vector3, duration: number, onArrive: () => void): Part {
	const projectile = new Instance("Part");
	projectile.Shape = Enum.PartType.Ball;
	projectile.Size = Vector3.one;
	projectile.Anchored = true;
	projectile.CanCollide = false;
	projectile.Material = Enum.Material.Neon;
	projectile.Parent = Workspace;
	const control = from.Lerp(to, 0.5).add(new Vector3(0, from.sub(to).Magnitude * 0.5, 0));
	const stops = [PALETTE.accent, PALETTE.danger, PALETTE.text];
	task.spawn(() => {
		let elapsed = 0;
		while (elapsed < duration) {
			elapsed += task.wait();
			const alpha = math.min(1, elapsed / duration);
			projectile.Position = bezier(from, control, to, alpha);
			projectile.Color = colorRamp(stops, alpha);
		}
		projectile.Destroy();
		onArrive();
	});
	return projectile;
}

export function flashScreen(color: Color3): void {
	const effect = new Instance("ColorCorrectionEffect");
	effect.TintColor = color;
	effect.Brightness = 0.3;
	effect.Parent = Lighting;
	const fade = TweenService.Create(effect, new TweenInfo(0.5), { Brightness: 0, TintColor: new Color3(1, 1, 1) });
	fade.Completed.Once(() => effect.Destroy());
	fade.Play();
}

export function setAtmosphere(density: number, color: Color3): Atmosphere {
	let atmosphere = Lighting.FindFirstChildOfClass("Atmosphere");
	if (!atmosphere) {
		atmosphere = new Instance("Atmosphere");
		atmosphere.Parent = Lighting;
	}
	atmosphere.Density = density;
	atmosphere.Color = color;
	atmosphere.Decay = color.Lerp(new Color3(0, 0, 0), 0.5);
	atmosphere.Glare = 0.2;
	atmosphere.Haze = 1.5;
	return atmosphere;
}

export function highlight(target: Instance, color: Color3, seconds: number): Highlight {
	const effect = new Instance("Highlight");
	effect.FillColor = color;
	effect.OutlineColor = color.Lerp(new Color3(1, 1, 1), 0.5);
	effect.FillTransparency = 0.6;
	effect.DepthMode = Enum.HighlightDepthMode.AlwaysOnTop;
	effect.Adornee = target;
	effect.Parent = target;
	Debris.AddItem(effect, seconds);
	return effect;
}

export function billboard(adornee: BasePart, text: string, color: Color3): BillboardGui {
	const gui = new Instance("BillboardGui");
	gui.Size = UDim2.fromOffset(200, 50);
	gui.StudsOffset = new Vector3(0, 3, 0);
	gui.AlwaysOnTop = true;
	gui.Adornee = adornee;
	const textLabel = new Instance("TextLabel");
	textLabel.Size = UDim2.fromScale(1, 1);
	textLabel.BackgroundTransparency = 1;
	textLabel.Text = text;
	textLabel.TextColor3 = color;
	textLabel.TextStrokeTransparency = 0.5;
	textLabel.TextScaled = true;
	textLabel.Font = Enum.Font.GothamBold;
	textLabel.Parent = gui;
	gui.Parent = adornee;
	TweenService.Create(gui, new TweenInfo(1.5), { StudsOffset: new Vector3(0, 6, 0) }).Play();
	TweenService.Create(textLabel, new TweenInfo(1.5), { TextTransparency: 1 }).Play();
	Debris.AddItem(gui, 1.6);
	return gui;
}
