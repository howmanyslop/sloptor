export default function foo(n: number): number {
	return n === 0 ? 1 : n * foo(n - 1);
}
