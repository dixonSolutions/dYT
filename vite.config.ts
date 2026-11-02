import { defineConfig } from 'vitest/config';
import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	// Tauri's CLI reads the dev server's output; keep errors visible and the port fixed.
	clearScreen: false,
	server: {
		port: 5173,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined,
		watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] }
	},
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// A single-page app: Tauri serves index.html for every route.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	test: {
		expect: { requireAssertions: true },
		projects: [
			{
				extends: './vite.config.ts',
				test: {
					name: 'server',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			}
		]
	}
});
