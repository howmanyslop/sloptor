// biome-ignore-all lint/suspicious/noShadowRestrictedNames: coalrule
// biome-ignore-all lint/suspicious/noEmptyInterface: coal rule

// The fixture is deliberately self-contained: no node_modules, no installed
// @rbxts type packages, no floating versions. `noLib` turns off the compiler's
// bundled standard library, so the handful of globals TS requires at startup
// are declared here as stubs. A real rbxts project gets these from
// @rbxts/compiler-types; declaring them locally keeps the benchmark's inputs
// byte-identical on every machine and in CI.
//
// Only members actually used by the corpus below are declared. Adding a method
// here changes what gets compiled, so edit it alongside the sources.

interface Array<T> {
	length: number;
	[index: number]: T;
	push(...items: T[]): number;
}
interface Boolean {}
interface CallableFunction extends Function {}
interface Function {}
interface IArguments {}
interface NewableFunction extends Function {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {
	length: number;
}
type Promise<T> = {};
type PromiseConstructor = {};
declare var Promise: PromiseConstructor;
declare function print(...params: Array<unknown>): void;
