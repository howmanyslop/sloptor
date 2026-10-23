declare function print(...params: Array<unknown>): void;

// noLib fundamental stubs (see asset_model/src/globals.d.ts). The $nameof,
// $keys and $file macros are provided by the compiler's
// synthetic __rotor_macros.d.ts injection — deliberately NOT declared here, so
// this fixture also exercises that injection.
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

// A nested object so $nameof(player.Humanoid.Health) has a typed expression.
interface Humanoid {
	Health: number;
}
interface Player {
	Humanoid: Humanoid;
}
declare const player: Player;
declare const foo: number;
