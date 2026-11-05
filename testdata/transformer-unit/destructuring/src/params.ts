function f([a, b]: [number, number]) {
	return a + b;
}
function g({ x, y }: { x: number; y: number }, scale = 2) {
	return (x + y) * scale;
}
function h(...[p, q]: [number, number]) {
	return p * q;
}
print(f([1, 2]), g({ x: 1, y: 2 }), h(3, 4));
