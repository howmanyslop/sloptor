declare function print(...params: Array<unknown>): void;

// Fundamental global types the checker resolves at initialization under
// noLib (real rbxts projects get these from @rbxts/compiler-types; stubbed
// here to keep the fixture self-contained). `$env` itself is deliberately
// NOT declared here: the compiler's synthetic __rotor_env.d.ts must provide
// it — that injection is part of what this fixture tests.
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Promise<T> {}
interface PromiseConstructor {}
declare var Promise: PromiseConstructor;
interface Object {}
interface RegExp {}
interface String {}
