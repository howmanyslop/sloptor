import { Controller, OnInit, OnStart } from "@flamework/core";
import { ValueController } from "./value.controller";

@Controller({ loadOrder: 2 })
export class ConsumerController implements OnInit, OnStart {
	public constructor(private readonly valueController: ValueController) {
		assert(valueController.value === "expected-controller-value", "controller constructor injection value mismatch");
		print("ASSERT controller constructor injected singleton/value");
	}

	public onInit(): void {
		print("ASSERT controller consumer init");
	}

	public onStart(): void {
		print("ASSERT controller consumer start");
	}
}
