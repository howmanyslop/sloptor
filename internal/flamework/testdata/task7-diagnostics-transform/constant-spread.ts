import { Networking } from "@flamework/networking";

interface ServerEvents {
	readonly submitted: (value: string) => void;
}

const event = Networking.createEvent<ServerEvents, Record<never, never>>();
const configuration = {};

event.createServer({ ...configuration });
