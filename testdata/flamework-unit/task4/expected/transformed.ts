//// src/server/class-decorator-di.server.ts
import { Reflect as Reflect, Flamework as Flamework } from "@flamework/core";
import { Dependency, Modding, OnStart, Service } from "@flamework/core";
const Tracked = Modding.createDecorator<[
    label: string
]>("Class", () => undefined);
export class LoggerService {
    public readonly value = "controlled";
    static {
        // (Flamework) LoggerService metadata
        Reflect["defineMetadata"](LoggerService, "identifier", "task4:server/class-decorator-di.server@LoggerService");
    }
}
// (Flamework) LoggerService decorators
Reflect["decorate"](LoggerService, "$:flamework@Service", Service, [
    { loadOrder: 1 }
]);
export class ConsumerService implements OnStart {
    public constructor(private readonly logger: LoggerService) { }
    public onStart(): void {
        print(this.logger.value);
    }
    static {
        // (Flamework) ConsumerService metadata
        Reflect["defineMetadata"](ConsumerService, "identifier", "task4:server/class-decorator-di.server@ConsumerService");
        Reflect["defineMetadata"](ConsumerService, "flamework:parameters", [
            "task4:server/class-decorator-di.server@LoggerService"
        ]);
        Reflect["defineMetadata"](ConsumerService, "flamework:implements", ["$:flamework@OnStart"]);
    }
}
// (Flamework) ConsumerService decorators
Reflect["decorate"](ConsumerService, "$:flamework@Service", Service, []);
Reflect["decorate"](ConsumerService, "task4:server/class-decorator-di.server@Tracked", Tracked, [
    "fixture"
]);
export const resolvedLogger = Flamework["resolveDependency"]<LoggerService>("task4:server/class-decorator-di.server@LoggerService" as never);

//// src/shared/component-attributes.ts
import { Reflect as Reflect } from "@flamework/core";
import { t as t } from "@flamework/core/out/prelude";
import { BaseComponent, Component } from "@flamework/components";
interface Attributes {
    readonly active: boolean;
    readonly label: string;
    readonly retries: number;
}
export class FixtureComponent extends BaseComponent<Attributes, Part> {
    static {
        // (Flamework) FixtureComponent metadata
        Reflect["defineMetadata"](FixtureComponent, "identifier", "task4:shared/component-attributes@FixtureComponent");
    }
}
// (Flamework) FixtureComponent decorators
Reflect["decorate"](FixtureComponent, "$c:components@Component", Component, [
    { tag: "Task4Fixture", "attributes": {
            "active": t["boolean"],
            "label": t["string"],
            "retries": t["number"]
        }, "instanceGuard": t["instanceIsA"]("Part") }
]);

//// src/shared/guards.ts
import { t as t } from "@flamework/core/out/prelude";
import { Flamework } from "@flamework/core";
interface Leaf {
    readonly id: string;
    readonly value: number;
}
interface Repeated {
    readonly first: Leaf;
    readonly second: Leaf;
    readonly third: Leaf;
}
const dedup = t["interface"]({
    "id": t["string"],
    "value": t["number"]
});
export const repeatedGuard = Flamework.createGuard<Repeated>(t["interface"]({
    "first": dedup,
    "second": dedup,
    "third": dedup
}) as never);
export const unionGuard = Flamework.createGuard<"ready" | number | undefined>(t["optional"](t["union"](t["number"], t["literal"]("ready"))) as never);
export const tupleGuard = Flamework.createGuard<readonly [
    string,
    number,
    boolean
]>(t["strictArray"](t["string"], t["number"], t["boolean"]) as never);

//// src/shared/macros.ts
import { t as t } from "@flamework/core/out/prelude";
import {} from "@flamework/core";
import { Flamework, Modding } from "@flamework/core";
export interface Payload {
    readonly count: number;
    readonly enabled: boolean;
    readonly label: string;
}
export const payloadId = "task4:shared/macros@Payload" as never as string;
export const payloadHash = "00000000-0000-4000-8000-000000000004" as never as string;
export const payloadGuard = Flamework.createGuard<Payload>(t["interface"]({
    "count": t["number"],
    "enabled": t["boolean"],
    "label": t["string"]
}) as never);
export const payloadShape = Modding.inspect<Modding.GenericMany<Payload, "id" | "text">>({ "id": "task4:shared/macros@Payload" as never, "text": "Payload" as never } as never);
export const tupleLabels = Modding.inspect<Modding.TupleLabels<[
    name: string,
    amount: number
]>>([
    "name" as never,
    "amount" as never
] as never);
export const callsite = Modding.inspect<Modding.CallerMany<"line" | "character" | "text" | "uuid">>({ "text": "Modding.inspect<Modding.CallerMany<\"line\" | \"character\" | \"text\" | \"uuid\">>()" as never, "line": 14 as never, "character": 25 as never, "uuid": "00000000-0000-4000-8000-000000000004" as never } as never);
Flamework["_addPaths"]([
    [
        "ServerScriptService",
        "TS"
    ]
] as never);
Flamework["_addPathsGlob"]("src/**/*.ts" as never);

//// src/shared/networking.ts
import { Networking } from "@flamework/networking";
interface ServerEvents {
    readonly submitted: (payload: {
        readonly id: string;
        readonly count: number;
    }) => void;
}
interface ClientEvents {
    readonly accepted: (id: string) => void;
}
interface ServerFunctions {
    readonly lookup: (id: string) => {
        readonly count: number;
    };
}
interface ClientFunctions {
    readonly confirm: (id: string) => boolean;
}
export const events = Networking.createEvent<ServerEvents, ClientEvents>("task4:shared/networking@events" as never);
export const functions = Networking.createFunction<ServerFunctions, ClientFunctions>("task4:shared/networking@functions" as never);
