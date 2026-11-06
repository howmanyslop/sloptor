export function formatDuration(seconds: number): string {
	const clamped = math.max(0, seconds);
	const minutes = math.floor(clamped / 60);
	const remainder = clamped - minutes * 60;
	if (minutes >= 60) {
		const hours = math.floor(minutes / 60);
		return string.format("%d:%02d:%05.2f", hours, minutes % 60, remainder);
	}
	return string.format("%d:%05.2f", minutes, remainder);
}

export function formatCount(value: number): string {
	const suffixes: ReadonlyArray<[threshold: number, suffix: string]> = [
		[1e9, "B"],
		[1e6, "M"],
		[1e3, "K"],
	];
	for (const [threshold, suffix] of suffixes) {
		if (math.abs(value) >= threshold) {
			return `${string.format("%.1f", value / threshold)}${suffix}`;
		}
	}
	return tostring(math.floor(value));
}

export function ordinal(position: number): string {
	const lastTwo = position % 100;
	if (lastTwo >= 11 && lastTwo <= 13) {
		return `${position}th`;
	}
	switch (position % 10) {
		case 1:
			return `${position}st`;
		case 2:
			return `${position}nd`;
		case 3:
			return `${position}rd`;
		default:
			return `${position}th`;
	}
}

export function padLeft(text: string, width: number, filler = " "): string {
	const missing = width - text.size();
	return missing > 0 ? `${filler.rep(missing)}${text}` : text;
}

export function dateKey(timestamp: number): string {
	const date = os.date("!*t", timestamp);
	return string.format("%04d-%02d-%02d", date.year, date.month, date.day);
}

export function percent(fraction: number, digits = 0): string {
	return `${string.format(`%.${digits}f`, fraction * 100)}%`;
}
