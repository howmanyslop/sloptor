import { Service } from "@flamework/core";

interface Required {
	required(): void;
}

/** @metadata reflect {@link Required constraint} */
interface Marked {}

@Service()
export class Invalid implements Marked {}
