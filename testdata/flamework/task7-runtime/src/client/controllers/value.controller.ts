import { Controller, OnInit, OnStart } from "@flamework/core";

@Controller({ loadOrder: 1 })
export class ValueController implements OnInit, OnStart {
	public readonly value = "expected-controller-value";

	public onInit(): void {
		print("ASSERT controller value init");
	}

	public onStart(): void {
		print("ASSERT controller value start");
	}
}
