// YouTube storyboards: sprite sheets of evenly spaced frames, used for the timeline strip
// and as the preview when a stream can't play in this webview.

import type { StoryboardLevel } from '#lib/api.js';

/** The level whose frames are closest to `width` pixels wide (preferring bigger). */
export function pickLevel(levels: StoryboardLevel[], width: number): StoryboardLevel | null {
	if (!levels.length) return null;
	return [...levels].sort((a, b) => Math.abs(a.width - width) - Math.abs(b.width - width) || b.width - a.width)[0];
}

export interface Frame {
	url: string;
	/** Sheet size in frames, for `background-size: cols*100% rows*100%`. */
	sheetW: number;
	sheetH: number;
	/** `background-position` percentages. */
	x: number;
	y: number;
}

export function frameAt(level: StoryboardLevel, t: number, duration: number): Frame {
	const interval = level.intervalMs > 0 ? level.intervalMs / 1000 : duration / level.count;
	const i = Math.max(0, Math.min(level.count - 1, Math.floor(t / Math.max(interval, 0.001))));
	const perSheet = level.cols * level.rows;
	const sheet = Math.floor(i / perSheet);
	const k = i % perSheet;
	const col = k % level.cols;
	const row = Math.floor(k / level.cols);
	// The last sheet can be shorter; rows past its end simply don't exist in the image.
	const rowsInSheet = sheet === Math.floor((level.count - 1) / perSheet) ? Math.ceil((level.count - sheet * perSheet) / level.cols) : level.rows;
	return {
		url: level.url.replace('$M', String(sheet)),
		sheetW: level.cols,
		sheetH: rowsInSheet,
		x: level.cols > 1 ? (col / (level.cols - 1)) * 100 : 0,
		y: rowsInSheet > 1 ? (row / (rowsInSheet - 1)) * 100 : 0
	};
}
