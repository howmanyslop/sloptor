type WritableProps<T extends Instance> = Partial<WritableInstanceProperties<T>>;

export function make<T extends keyof CreatableInstances>(
	className: T,
	props: WritableProps<CreatableInstances[T]> = {},
	children: ReadonlyArray<Instance> = [],
): CreatableInstances[T] {
	const instance = new Instance(className);
	for (const [key, value] of pairs(props as object)) {
		if (key === "Parent") continue;
		(instance as unknown as Record<string, unknown>)[key as string] = value;
	}
	for (const child of children) child.Parent = instance;
	const parent = (props as { Parent?: Instance }).Parent;
	if (parent) instance.Parent = parent;
	return instance;
}

export function makePart(name: string, size: Vector3, cframe: CFrame, color: Color3, parent: Instance): Part {
	const part = new Instance("Part");
	part.Name = name;
	part.Size = size;
	part.CFrame = cframe;
	part.Color = color;
	part.Anchored = true;
	part.TopSurface = Enum.SurfaceType.Smooth;
	part.BottomSurface = Enum.SurfaceType.Smooth;
	part.Material = Enum.Material.SmoothPlastic;
	part.Parent = parent;
	return part;
}

export function getOrCreateFolder(parent: Instance, name: string): Folder {
	const existing = parent.FindFirstChild(name);
	if (existing && existing.IsA("Folder")) return existing;
	const folder = new Instance("Folder");
	folder.Name = name;
	folder.Parent = parent;
	return folder;
}

export function weld(first: BasePart, second: BasePart): WeldConstraint {
	const constraint = new Instance("WeldConstraint");
	constraint.Part0 = first;
	constraint.Part1 = second;
	constraint.Parent = first;
	return constraint;
}

export function addLight(part: BasePart, color: Color3, range: number): PointLight {
	const light = new Instance("PointLight");
	light.Color = color;
	light.Range = range;
	light.Brightness = 2;
	light.Shadows = true;
	light.Parent = part;
	return light;
}

export function addParticles(part: BasePart, color: Color3): ParticleEmitter {
	const emitter = new Instance("ParticleEmitter");
	emitter.Color = new ColorSequence(color, color.Lerp(new Color3(1, 1, 1), 0.5));
	emitter.Size = new NumberSequence([new NumberSequenceKeypoint(0, 0.5), new NumberSequenceKeypoint(1, 0)]);
	emitter.Transparency = new NumberSequence(0, 1);
	emitter.Lifetime = new NumberRange(0.5, 1.5);
	emitter.Rate = 20;
	emitter.Speed = new NumberRange(2, 6);
	emitter.SpreadAngle = new Vector2(180, 180);
	emitter.Parent = part;
	return emitter;
}

export function cloneInto<T extends Instance>(template: T, parent: Instance, cframe?: CFrame): T {
	const copy = template.Clone();
	if (cframe && copy.IsA("BasePart")) copy.CFrame = cframe;
	else if (cframe && copy.IsA("Model")) copy.PivotTo(cframe);
	copy.Parent = parent;
	return copy;
}

export function setAttributes(instance: Instance, attributes: Record<string, AttributeValue>): void {
	for (const [key, value] of pairs(attributes)) instance.SetAttribute(key as string, value);
}

export function readNumberAttribute(instance: Instance, name: string, fallback: number): number {
	const value = instance.GetAttribute(name);
	return typeIs(value, "number") ? value : fallback;
}
