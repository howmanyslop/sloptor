import type { ReactNode } from "react";

interface OverprintProperties {
	value: number;
}

export function Overprint({ value }: Readonly<OverprintProperties>): ReactNode {
	const numeral = String(value);
	// Plex Mono's dotted zero muddies under the halo, so a numeral
	// with a zero prints in Plex Sans instead.
	const className = numeral.includes("0") ? "overprint overprint--sans" : "overprint";
	return (
		<span className={className} data-value={numeral}>
			{numeral}
		</span>
	);
}
