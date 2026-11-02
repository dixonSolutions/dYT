// Numbers people read: times, sizes, speeds.

/** `1:02:03.4`, `3:05`, `0:07.25`; `digits` fractional digits at most (trailing zeros dropped). */
export function clock(t: number, digits = 2): string {
	if (!Number.isFinite(t) || t < 0) t = 0;
	const scale = 10 ** digits;
	const total = Math.round(t * scale) / scale;
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = Math.floor(total % 60);
	const frac = digits > 0 ? (total % 1).toFixed(digits).slice(1).replace(/\.?0+$/, '') : '';
	const ss = String(s).padStart(2, '0') + frac;
	return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** Parse `1:02:03.5`, `2:05`, `75`, `75.5`, `1m15s`; `null` when it isn't a time. */
export function parseClock(text: string): number | null {
	const s = text.trim();
	if (!s) return null;
	const hms = s.match(/^(?:(\d+)h)?\s*(?:(\d+)m)?\s*(?:(\d+(?:\.\d+)?)s)?$/i);
	if (hms && (hms[1] || hms[2] || hms[3])) return (+(hms[1] ?? 0)) * 3600 + (+(hms[2] ?? 0)) * 60 + +(hms[3] ?? 0);
	const parts = s.split(':');
	if (parts.length > 3 || parts.some((p) => !/^\d+(\.\d+)?$/.test(p.trim()))) return null;
	let t = 0;
	for (const [i, p] of parts.entries()) {
		const v = parseFloat(p);
		if (i > 0 && v >= 60) return null;
		t = t * 60 + v;
	}
	return t;
}

export function bytes(n: number | null | undefined): string {
	if (n == null || !Number.isFinite(n)) return '';
	if (n < 1e3) return `${Math.round(n)} B`;
	if (n < 1e6) return `${(n / 1e3).toFixed(0)} kB`;
	if (n < 1e9) return `${(n / 1e6).toFixed(n < 1e7 ? 1 : 0)} MB`;
	return `${(n / 1e9).toFixed(2)} GB`;
}

export function speed(bps: number): string {
	if (!bps) return '';
	return `${bytes(bps)}/s`;
}

export function eta(seconds: number | null): string {
	if (seconds == null || !Number.isFinite(seconds)) return '';
	if (seconds < 1) return 'a moment left';
	if (seconds < 60) return `${Math.ceil(seconds)} s left`;
	if (seconds < 3600) return `${Math.ceil(seconds / 60)} min left`;
	return `${(seconds / 3600).toFixed(1)} h left`;
}

export function views(n: number | null): string {
	if (n == null) return '';
	const f = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 });
	return `${f.format(n)} views`;
}

export function ago(ms: number, now = Date.now()): string {
	const s = Math.max(0, (now - ms) / 1000);
	const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
	if (s < 45) return 'just now';
	if (s < 3600) return rtf.format(-Math.round(s / 60), 'minute');
	if (s < 86400) return rtf.format(-Math.round(s / 3600), 'hour');
	if (s < 86400 * 30) return rtf.format(-Math.round(s / 86400), 'day');
	return new Date(ms).toLocaleDateString();
}

/** The last path component, for showing a folder or file without the whole path. */
export function basename(path: string): string {
	const parts = path.split(/[\\/]/).filter(Boolean);
	return parts.at(-1) ?? path;
}

/** `~/Downloads` instead of `/home/me/Downloads`, `Download` on Android's shared storage. */
export function prettyPath(path: string): string {
	return path.replace(/^\/(?:var\/)?home\/[^/]+/, '~').replace(/^\/storage\/emulated\/0\/?/, 'Phone/');
}
