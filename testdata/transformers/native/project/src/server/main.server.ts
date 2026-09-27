import { Flamework } from "@flamework/core";

function startup() {
	print("before addPaths");
	Flamework.addPaths("src/server/services");
	print("after addPaths");
	Flamework.ignite();
}

startup();
