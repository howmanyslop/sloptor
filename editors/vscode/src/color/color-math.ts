export type ColorTriple = readonly [number, number, number];

const MAXIMUM_PRECISION = 6;

/**
 * Clamps a component to the 0..1 range.
 *
 * @param value - The component to clamp.
 * @returns The component, clamped to 0..1.
 */
export function clampUnit(value: number): number {
	return Math.min(Math.max(value, 0), 1);
}

/**
 * Clamps a decimal-place count to the integers 0..6 the picker writes. NaN
 * becomes 0.
 *
 * @param precision - The requested number of decimal places.
 * @returns A whole number of decimal places between 0 and 6.
 */
export function clampPrecision(precision: number): number {
	if (precision >= MAXIMUM_PRECISION) return MAXIMUM_PRECISION;
	return precision > 0 ? Math.trunc(precision) : 0;
}

/**
 * Rounds a value to a number of decimal places.
 *
 * @param value - Any finite number.
 * @param precision - Decimal places to keep, already clamped.
 * @returns The rounded value.
 */
export function roundToPrecision(value: number, precision: number): number {
	const scale = 10 ** precision;
	return Math.round(value * scale) / scale;
}

/**
 * Converts HSV to RGB. Every component is 0..1, hue included, the same as
 * `Color3.fromHSV`. Out-of-range input is clamped.
 *
 * @param hue - Hue, 0..1. 1 wraps to red, like 0.
 * @param saturation - Saturation, 0..1.
 * @param value - Value, 0..1.
 * @returns Red, green, and blue, each 0..1.
 */
export function convertHsvToRgb(hue: number, saturation: number, value: number): ColorTriple {
	const sector = clampUnit(hue) * 6;
	const whole = Math.floor(sector);
	const fraction = sector - whole;
	const chroma = clampUnit(saturation);
	const brightness = clampUnit(value);

	const low = brightness * (1 - chroma);
	const falling = brightness * (1 - fraction * chroma);
	const rising = brightness * (1 - (1 - fraction) * chroma);

	switch (whole % 6) {
		case 0:
			return [brightness, rising, low];

		case 1:
			return [falling, brightness, low];

		case 2:
			return [low, brightness, rising];

		case 3:
			return [low, falling, brightness];

		case 4:
			return [rising, low, brightness];

		default:
			return [brightness, low, falling];
	}
}

/**
 * Converts RGB to HSV. Every component is 0..1, hue included. A gray has hue
 * 0.
 *
 * @param red - Red, 0..1.
 * @param green - Green, 0..1.
 * @param blue - Blue, 0..1.
 * @returns Hue, saturation, and value, each 0..1.
 */
export function convertRgbToHsv(red: number, green: number, blue: number): ColorTriple {
	const maximum = Math.max(red, green, blue);
	const delta = maximum - Math.min(red, green, blue);
	if (delta === 0) {
		return [0, 0, maximum];
	}

	const saturation = delta / maximum;
	if (maximum === red) {
		const hue = (green - blue) / delta;
		return [(hue < 0 ? hue + 6 : hue) / 6, saturation, maximum];
	}

	if (maximum === green) {
		return [((blue - red) / delta + 2) / 6, saturation, maximum];
	}

	return [((red - green) / delta + 4) / 6, saturation, maximum];
}
