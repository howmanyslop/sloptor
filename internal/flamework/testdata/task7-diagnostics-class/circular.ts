import { Service } from "@flamework/core";

@Service()
export class Alpha {
	public constructor(beta: Beta) {}
}

@Service()
export class Beta {
	public constructor(alpha: Alpha) {}
}
