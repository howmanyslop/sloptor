interface Configuration {
	readonly middleware: number;
}

/** @metadata macro {@link config intrinsic-middleware} */
declare function createServer(config: Configuration): void;

createServer({ middleware: 1 });
