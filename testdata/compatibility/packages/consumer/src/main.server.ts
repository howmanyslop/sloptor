import { callback, direct, object } from "@rbxts/compat-library";

assert(callback(41) === 42, "imported callback shifted its argument");
assert(direct(41) === 42, "imported receiver-bearing function shifted its argument");
assert(object.method(32) === 42, "imported method lost its receiver");

const wrapped = { callback };
assert(wrapped.callback(41) === 42, "imported callback changed convention through a property");
