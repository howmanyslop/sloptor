import { OnInit, OnStart, Service } from "@flamework/core";

@Service({ loadOrder: 1 })
export class ValueService implements OnInit, OnStart {
	public readonly value = "expected-service-value";

	public onInit(): void {
		print("ASSERT service value init");
	}

	public onStart(): void {
		print("ASSERT service value start");
	}
}
