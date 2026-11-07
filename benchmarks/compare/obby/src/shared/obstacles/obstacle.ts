import type { Bounds, ObstacleKind, ObstacleParams, ObstacleSpec } from "../types";

export interface BuildContext {
	readonly parent: Instance;
	readonly origin: CFrame;
	readonly accent: Color3;
	readonly cellSize: number;
}

export interface PartOptions {
	readonly name: string;
	readonly size: Vector3;
	readonly cframe: CFrame;
	readonly color?: Color3;
	readonly material?: Enum.Material;
	readonly transparency?: number;
	readonly canCollide?: boolean;
	readonly anchored?: boolean;
	readonly shape?: Enum.PartType;
}

export type HazardResponse = "kill" | "push" | "none";

export abstract class Obstacle<P extends ObstacleParams = ObstacleParams> {
	private static nextSerial = 1;

	public readonly serial: number;
	protected model?: Model;
	protected elapsed = 0;
	private active = true;

	protected constructor(
		public readonly spec: ObstacleSpec,
		protected readonly params: P,
	) {
		this.serial = Obstacle.nextSerial++;
	}

	public static resetSerials(): void {
		Obstacle.nextSerial = 1;
	}

	public abstract getKind(): ObstacleKind;

	/** Local-space bounds relative to the obstacle origin. */
	public abstract getLocalBounds(): Bounds;

	public abstract getHazardResponse(): HazardResponse;

	/** Rough contribution to stage difficulty, before tier scaling. */
	public abstract getDifficultyWeight(): number;

	protected abstract buildParts(context: BuildContext, model: Model): void;

	public build(context: BuildContext): Model {
		const model = new Instance("Model");
		model.Name = `${this.getKind()}_${this.spec.id}`;
		model.SetAttribute("ObstacleKind", this.getKind());
		model.SetAttribute("ObstacleSerial", this.serial);
		model.SetAttribute("Hazard", this.getHazardResponse() !== "none");
		this.buildParts(context, model);
		model.Parent = context.parent;
		this.model = model;
		return model;
	}

	public step(deltaTime: number): void {
		if (!this.active || this.model === undefined) {
			return;
		}
		this.elapsed += deltaTime;
		this.animate(this.elapsed, deltaTime);
	}

	public isActive(): boolean {
		return this.active;
	}

	public setActive(active: boolean): void {
		this.active = active;
	}

	public getModel(): Model | undefined {
		return this.model;
	}

	public getElapsed(): number {
		return this.elapsed;
	}

	public describe(): string {
		const [x, y, z] = this.spec.cell;
		return `${this.getKind()}#${this.serial} @(${x}, ${y}, ${z}) weight=${string.format("%.2f", this.getDifficultyWeight())}`;
	}

	public destroy(): void {
		this.active = false;
		this.model?.Destroy();
		this.model = undefined;
	}

	protected animate(_time: number, _deltaTime: number): void {}

	protected createPart(model: Model, options: PartOptions): Part {
		const part = new Instance("Part");
		part.Name = options.name;
		part.Size = options.size;
		part.CFrame = options.cframe;
		part.Anchored = options.anchored ?? true;
		part.CanCollide = options.canCollide ?? true;
		part.Material = options.material ?? Enum.Material.SmoothPlastic;
		part.Color = options.color ?? Color3.fromRGB(200, 200, 200);
		part.Transparency = options.transparency ?? 0;
		part.TopSurface = Enum.SurfaceType.Smooth;
		part.BottomSurface = Enum.SurfaceType.Smooth;
		if (options.shape !== undefined) {
			part.Shape = options.shape;
		}
		part.Parent = model;
		if (model.PrimaryPart === undefined) {
			model.PrimaryPart = part;
		}
		return part;
	}

	protected tagHazard(part: BasePart, response: HazardResponse = this.getHazardResponse()): void {
		part.SetAttribute("HazardResponse", response);
	}

	protected originFor(context: BuildContext): CFrame {
		return context.origin.mul(CFrame.Angles(0, math.rad(this.spec.rotation), 0));
	}
}
