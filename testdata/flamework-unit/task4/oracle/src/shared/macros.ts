import { Flamework, Modding } from "@flamework/core";

export interface Payload {
	readonly count: number;
	readonly enabled: boolean;
	readonly label: string;
}

export const payloadId = Flamework.id<Payload>();
export const payloadHash = Flamework.hash<"payload", "task4">();
export const payloadGuard = Flamework.createGuard<Payload>();
export const payloadShape = Modding.inspect<Modding.GenericMany<Payload, "id" | "text">>();
export const tupleLabels = Modding.inspect<Modding.TupleLabels<[name: string, amount: number]>>();
export const callsite = Modding.inspect<Modding.CallerMany<"line" | "character" | "text" | "uuid">>();

Flamework.addPaths("src/server");
Flamework.addPathsGlob("src/**/*.ts");
