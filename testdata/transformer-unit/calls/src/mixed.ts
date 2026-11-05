interface MixedFn {
	(this: void, x: number): void;
	(this: Mixed, x: number): void;
}
interface Mixed {
	fn: MixedFn;
}
declare const mixed: Mixed;
mixed.fn(1);
