// The Android activity's bridge (MainActivity.kt): shared links, the system scheme, and the
// media store. Absent everywhere else.

interface Bridge {
	takeLink(): string | null;
	isDark(): boolean;
	scanFiles(json: string): void;
	openFile(path: string): void;
}

export type AndroidEvent = { type: 'link'; link: string } | { type: 'theme'; dark: boolean };

export const android: Bridge | null = typeof window !== 'undefined' ? ((window as unknown as { DytAndroid?: Bridge }).DytAndroid ?? null) : null;

export function onAndroid(fn: (e: AndroidEvent) => void): () => void {
	const h = (e: Event) => fn((e as CustomEvent<AndroidEvent>).detail);
	window.addEventListener('dyt-android', h);
	return () => window.removeEventListener('dyt-android', h);
}
