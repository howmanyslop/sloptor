-- Definitions the luauTypes analyzer harness loads through
-- `luau-lsp analyze --definitions=`. It covers only the globals compiled
-- fixtures reach for: the RuntimeLib table (`TS`), `script`, `game`, `Enum`,
-- and a handful of Roblox extern types with a few members each. Fixtures
-- under src/ must stay inside this surface; anything else is an unknown
-- global at analysis time. Members typed `any` are deliberate: the harness
-- checks sloptor's annotations, not RuntimeLib's.
--
-- Under luau-lsp's default (roblox) platform, every extern type named
-- `Enum<Name>` is also reachable as the type `Enum.<Name>`; the standard
-- platform drops that namespace, so stub-backed runs keep the default.
declare TS: {
	import: (script: any, ...any) -> any,
	getModule: (script: any, scope: string, name: string) -> any,
	async: (f: any) -> any,
	await: (value: any) -> any,
	generator: (f: any) -> any,
	instanceof: (object: any, class: any) -> boolean,
	Promise: any,
	try: any,
	TRY_RETURN: number,
	TRY_BREAK: number,
	TRY_CONTINUE: number,
}

declare extern type Instance with
	Name: string
	ClassName: string
	Parent: Instance?
	function IsA(self, className: string): boolean
end

declare extern type DataModel extends Instance with
	function GetService(self, name: string): Instance
end

declare extern type Vector3 with
	X: number
	Y: number
	Z: number
	Magnitude: number
	Unit: Vector3
	function Dot(self, other: Vector3): number
end

declare extern type CFrame with
	Position: Vector3
	function ToWorldSpace(self, other: CFrame): CFrame
end

declare extern type EnumItem with
	Name: string
	Value: number
end

declare extern type EnumMaterial extends EnumItem with
end

declare extern type EnumMaterial_INTERNAL with
	Plastic: EnumMaterial
	Wood: EnumMaterial
	Metal: EnumMaterial
	function GetEnumItems(self): { EnumMaterial }
end

declare extern type Enums with
	Material: EnumMaterial_INTERNAL
end

declare extern type Part extends Instance with
	Anchored: boolean
	CFrame: CFrame
	Position: Vector3
	Size: Vector3
	Material: EnumMaterial
	Transparency: number
end

declare Vector3: {
	new: (x: number?, y: number?, z: number?) -> Vector3,
	zero: Vector3,
	one: Vector3,
}

declare CFrame: {
	new: (x: number?, y: number?, z: number?) -> CFrame,
	identity: CFrame,
}

declare Enum: Enums
declare script: Instance
declare game: DataModel
