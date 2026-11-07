import { useCallback, useMemo } from "react";
import { Toggle } from "@base-ui/react/toggle";
import { ToggleGroup } from "@base-ui/react/toggle-group";

import type { ReactNode } from "react";

interface SegmentedOption {
	readonly label: string;
	readonly value: string;
}

interface SegmentedControlProperties {
	label: string;
	onChange: (value: string) => void;
	options: ReadonlyArray<SegmentedOption>;
	value: string;
}

/**
 * One choice from a few, drawn as outlined segments. Base UI supplies the
 * keyboard handling and ARIA. Pressing the selected segment again keeps it.
 *
 * @param properties - The accessible label, options, and selected value.
 * @returns A row of outlined segments with the selected one filled.
 */
export function SegmentedControl({ label, onChange, options, value }: Readonly<SegmentedControlProperties>): ReactNode {
	const pressed = useMemo(() => [value], [value]);
	const handleChange = useCallback(
		([next]: ReadonlyArray<string>): void => {
			if (next !== undefined) onChange(next);
		},
		[onChange],
	);

	const toggles = new Array<ReactNode>();
	for (const { label: text, value: key } of options) {
		toggles.push(
			<Toggle key={key} value={key}>
				{text}
			</Toggle>,
		);
	}

	return (
		<div className="segmented">
			<ToggleGroup aria-label={label} value={pressed} onValueChange={handleChange}>
				{toggles}
			</ToggleGroup>
		</div>
	);
}
