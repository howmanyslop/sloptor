import { OnStart, Service } from "@flamework/core";

@Service()
export class TestService implements OnStart {
	public onStart(): void {
		print("TestService started");
	}
}
