import { Flamework } from "@flamework/core";

interface Payload {
	readonly value: string;
}

export const payloadId = Flamework.id<Payload>();
export const payloadGuard = Flamework.createGuard<Payload>();
