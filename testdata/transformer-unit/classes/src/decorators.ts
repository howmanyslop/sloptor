function Component(ctor: defined) {
	print("Component", ctor);
}

function ComponentFactory(arg: string) {
	return (ctor: defined) => {
		print("ComponentFactory", arg, ctor);
	};
}

function LogProp(target: defined, key: string) {
	print("LogProp", target, key);
}

function LogMethod(target: defined, key: string, descriptor: { value: unknown }) {
	print("LogMethod", target, key, descriptor);
}

function LogParam(target: defined, key: string | undefined, index: number) {
	print("LogParam", target, key, index);
}

let mutableDeco = (ctor: defined) => {
	print("mutable", ctor);
};

@Component
@ComponentFactory("hi")
class Decorated {
	@LogProp
	value = 1;
	@LogProp
	static svalue = 2;
	@LogMethod
	method(@LogParam a: number, @LogParam b: string) {
		print(a, b);
	}
	@LogMethod
	static smethod() {}
	constructor(@LogParam x: number) {
		print(x);
	}
}

@mutableDeco
@Component
class MutableDecorated {}

const KEY = "dyn";
class ComputedDecorated {
	@LogMethod
	[KEY]() {}
}

@Component
export class Exported {
	@LogMethod
	@LogMethod
	multi() {}
}

print(Decorated, MutableDecorated, ComputedDecorated, Exported, mutableDeco);
