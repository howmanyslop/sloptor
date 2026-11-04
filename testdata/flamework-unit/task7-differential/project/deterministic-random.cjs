"use strict";

const crypto = require("node:crypto");
const uuidPath = require.resolve("uuid");
const uuid = require(uuidPath);

const controlledUUID = "00000000-0000-4000-8000-000000000004";

crypto.randomUUID = () => controlledUUID;
Math.random = () => 0;
require.cache[uuidPath].exports = new Proxy(uuid, {
	get(target, property, receiver) {
		return property === "v4" ? () => controlledUUID : Reflect.get(target, property, receiver);
	},
});
