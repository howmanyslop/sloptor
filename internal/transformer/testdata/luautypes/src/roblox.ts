// Placeholder for WP6: Roblox data types and an enum item as parameters and
// returns. Every global used here is declared in ../sloptor.d.luau.
export function offset(position: Vector3, delta: Vector3): Vector3 {
	return position.add(delta);
}

export function anchor(part: Part, material: Enum.Material): Part {
	part.Anchored = true;
	part.Material = material;
	return part;
}

export function plastic(part: Part): Part {
	return anchor(part, Enum.Material.Plastic);
}

export function magnitude(part: Part): number {
	return part.Position.Magnitude;
}
