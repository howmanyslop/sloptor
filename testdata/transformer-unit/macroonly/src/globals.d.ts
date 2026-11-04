// Checker-light stand-in for an @rbxts/compiler-types NEWER than the
// compiler: the global (macro-only) ReadonlySet interface declares a method
// the compiler has no macro for. The interface must have a SINGLE global
// declaration — upstream's getPropertyCallMacro identity check
// (`symbols.get(symbol.parent.name) === symbol.parent`) only holds for an
// unmerged global symbol; a user-side augmentation merges into a transient
// clone and upstream (and rotor) falls back to a plain method call instead.
interface Array<T> {}

interface ReadonlySet<T> {
	size(this: ReadonlySet<T>): number;
	union(this: ReadonlySet<T>, other: ReadonlySet<T>): ReadonlySet<T>;
}

declare const setA: ReadonlySet<number>;
declare const setB: ReadonlySet<number>;
declare function print(...params: Array<unknown>): void;
