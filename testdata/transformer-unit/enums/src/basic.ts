enum Fruit {
	Apple,
	Banana,
	Cherry,
}
enum Mixed {
	A = 5,
	B,
	C = 10,
}
enum Color {
	Red = "red",
	Green = "green",
}
enum Hetero {
	Num = 1,
	Str = "str",
}
const base = 10;
enum Computed {
	X = base * 2,
	Y = "y".size(),
}
const enum Direction {
	Up,
	Down,
}
declare enum Ambient {
	A,
}
export enum Exported {
	E1,
	E2,
}
enum Weird {
	["a b"] = 1,
	ok = 2,
}
export function uses() {
	print(Fruit.Apple, Fruit[1], Mixed.B, Color.Red, Hetero.Str, Computed.X, Computed.Y, Weird.ok);
	print(Direction.Up, Direction.Down);
	print(Exported.E2);
}
