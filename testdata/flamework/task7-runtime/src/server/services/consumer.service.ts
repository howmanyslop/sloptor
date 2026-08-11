import { OnInit, OnStart, Service } from "@flamework/core";
import { ValueService } from "./value.service";

@Service({ loadOrder: 2 })
export class ConsumerService implements OnInit, OnStart {
	public constructor(private readonly valueService: ValueService) {
		assert(valueService.value === "expected-service-value", "service constructor injection value mismatch");
		print("ASSERT service constructor injected singleton/value");
	}

	public onInit(): void {
		print("ASSERT service consumer init");
	}

	public onStart(): void {
		print("ASSERT service consumer start");
	}
}
