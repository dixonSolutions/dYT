// Light or dark, always the system's choice, applied live.
//
// Linux: the backend relays the desktop portal's color-scheme (`system-theme` events).
// Everywhere: the webview's `prefers-color-scheme`, which also covers Android and browsers.

import { listen } from '@tauri-apps/api/event';
import { api, inTauri } from './api';

class Theme {
	dark = $state(false);
	/** Set once the portal has spoken; from then on it wins over the media query. */
	#fromPortal = false;

	start() {
		const mq = matchMedia('(prefers-color-scheme: dark)');
		this.#apply(mq.matches, false);
		mq.addEventListener('change', (e) => {
			if (!this.#fromPortal) this.#apply(e.matches, true);
		});
		if (inTauri) {
			api.systemTheme().then((t) => t && this.#portal(t, false));
			listen<'dark' | 'light'>('system-theme', (e) => this.#portal(e.payload, true));
		}
	}

	#portal(t: 'dark' | 'light', animate: boolean) {
		this.#fromPortal = true;
		this.#apply(t === 'dark', animate);
	}

	#apply(dark: boolean, animate: boolean) {
		if (this.dark === dark && document.documentElement.classList.contains('dark') === dark) return;
		this.dark = dark;
		const root = document.documentElement;
		if (animate) {
			root.classList.add('theme-fade');
			setTimeout(() => root.classList.remove('theme-fade'), 260);
		}
		root.classList.toggle('dark', dark);
		document.querySelectorAll('meta[name="theme-color"]').forEach((m) => m.setAttribute('content', getComputedStyle(root).getPropertyValue('--background').trim() || (dark ? '#151213' : '#ffffff')));
	}
}

export const theme = new Theme();
