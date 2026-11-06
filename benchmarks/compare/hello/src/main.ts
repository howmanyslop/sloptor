import { Players } from "@rbxts/services";

export function greet(name: string): string {
	return `hello ${name}`;
}

for (const player of Players.GetPlayers()) {
	print(greet(player.Name));
}
