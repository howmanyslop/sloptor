import { Dependency, Modding, OnStart, Service } from "@flamework/core";

const Tracked = Modding.createDecorator<[label: string]>("Class", () => undefined);

@Service({ loadOrder: 1 })
export class LoggerService {
	public readonly value = "controlled";
}

@Tracked("fixture")
@Service()
export class ConsumerService implements OnStart {
	public constructor(private readonly logger: LoggerService) {}

	public onStart(): void {
		print(this.logger.value);
	}
}

export const resolvedLogger = Dependency<LoggerService>();
