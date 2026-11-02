// End-to-end: the real app (WebKitGTK + the Rust engine) driven over WebDriver, downloading
// from YouTube for real. Run it with tests/e2e/run.sh, which starts a headless compositor,
// tauri-driver and a throwaway data dir.

import { mkdirSync, readdirSync, renameSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';

const DRIVER = process.env.DRIVER ?? 'http://127.0.0.1:4444';
const APP = resolve(process.env.APP ?? 'target/debug/dyt');
const OUT = resolve(process.env.OUT ?? '.scratch/e2e');
const DOWNLOADS = join(OUT, 'downloads');
mkdirSync(DOWNLOADS, { recursive: true });

async function wd(method, path, body) {
	const r = await fetch(DRIVER + path, {
		method,
		headers: { 'content-type': 'application/json' },
		body: body ? JSON.stringify(body) : undefined,
		signal: AbortSignal.timeout(60000)
	});
	const j = await r.json();
	if (j.value && j.value.error) throw new Error(`${path}: ${j.value.error}: ${j.value.message}`);
	return j.value;
}

const session = await wd('POST', '/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: APP } } } });
const sid = session.sessionId;
const S = (p) => `/session/${sid}${p}`;
const js = (script, ...args) => wd('POST', S('/execute/sync'), { script, args });
const jsAsync = (script, ...args) => wd('POST', S('/execute/async'), { script, args });
// WebKit's own WebDriver screenshot hangs on a headless compositor, so the compositor takes them.
let shots = 0;
async function shot(name) {
	const dir = join(OUT, 'shots');
	mkdirSync(dir, { recursive: true });
	await new Promise((r) => setTimeout(r, 400));
	const before = new Set(readdirSync(dir));
	execFileSync('weston-screenshooter', { cwd: dir, stdio: 'ignore' });
	const made = readdirSync(dir).find((f) => !before.has(f) && f.startsWith('wayland-screenshot'));
	if (made) renameSync(join(dir, made), join(dir, `${String(++shots).padStart(2, '0')}-${name}.png`));
}
async function until(what, fn, timeout = 30000) {
	const t0 = Date.now();
	for (;;) {
		const v = await fn();
		if (v) return v;
		if (Date.now() - t0 > timeout) throw new Error(`timed out waiting for ${what}`);
		await new Promise((r) => setTimeout(r, 250));
	}
}
const text = () => js('return document.body.innerText');
/** Real keystrokes into a field (cleared first); `\uE007` is Enter. */
async function keys(selector, text) {
	const el = await wd('POST', S('/element'), { using: 'css selector', value: selector });
	const id = Object.values(el)[0];
	await wd('POST', S(`/element/${id}/clear`), {});
	await wd('POST', S(`/element/${id}/value`), { text });
}
/** A real click through WebKit's automation (pointer events and all), like a user's. */
async function press(selector) {
	const el = await wd('POST', S('/element'), { using: 'css selector', value: selector });
	await wd('POST', S(`/element/${Object.values(el)[0]}/click`), {});
}
const click = (selector) => js('const e = document.querySelector(arguments[0]); if (!e) throw new Error("no " + arguments[0]); e.click(); return true', selector);
const clickText = (tag, label) =>
	js(
		`const e = [...document.querySelectorAll(arguments[0])].find(e => (e.getAttribute('aria-label') || e.innerText).replace(/\\s+/g, ' ').trim().startsWith(arguments[1])); if (!e) throw new Error('no ' + arguments[1]); e.click(); return true`,
		tag,
		label
	);
async function typeInto(selector, value) {
	// Through the input's own setter so Svelte's binding sees it, like a user typing.
	await js(
		`const e = document.querySelector(arguments[0]); e.focus(); const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; set.call(e, arguments[1]); e.dispatchEvent(new Event('input', { bubbles: true })); return true`,
		selector,
		value
	);
}
const invoke = (cmd, args) => jsAsync(`const done = arguments[arguments.length - 1]; window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]).then(v => done({ ok: v }), e => done({ err: e }))`, cmd, args ?? {});

const results = [];
/** Back to the main screen: no dialog or popover left open by an earlier step. */
async function reset() {
	for (let i = 0; i < 3; i++) {
		const open = await js(`return !!document.querySelector('[role="dialog"], [data-popover-content], [data-slot="popover-content"]')`);
		if (!open) return;
		await wd('POST', S('/actions'), { actions: [{ type: 'key', id: 'kb', actions: [{ type: 'keyDown', value: '\uE00C' }, { type: 'keyUp', value: '\uE00C' }] }] }).catch(() => {});
		await new Promise((r) => setTimeout(r, 300));
	}
}
async function step(name, fn, { soft = false } = {}) {
	const t0 = Date.now();
	try {
		await reset();
		await fn();
		results.push(['ok', name, Date.now() - t0]);
		console.log(`ok    ${name} (${((Date.now() - t0) / 1000).toFixed(1)}s)`);
	} catch (e) {
		results.push([soft ? 'WARN' : 'FAIL', name, Date.now() - t0]);
		console.log(`${soft ? 'WARN' : 'FAIL'}  ${name}: ${e.message}`);
		await shot(`fail-${name.replace(/\W+/g, '-')}`).catch(() => {});
	}
}

function newFiles(before) {
	return readdirSync(DOWNLOADS).filter((f) => !before.includes(f) && !f.startsWith('.'));
}
function probe(file) {
	try {
		const o = execFileSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration:stream=codec_name', '-of', 'json', join(DOWNLOADS, file)]).toString();
		const j = JSON.parse(o);
		return { duration: +j.format.duration, codecs: j.streams.map((s) => s.codec_name) };
	} catch {
		return null;
	}
}
async function waitForJob() {
	return until('the download to finish', async () => {
		const t = await text();
		if (/failed|went wrong/i.test(t) && !/Download finished/.test(t)) {
			const err = await js(`return [...document.querySelectorAll('li')].map(l => l.innerText).join(' | ')`);
			throw new Error('download failed: ' + err);
		}
		return /\d+ (s|MB|kB) in [\d.]+ s/.test(t) && !(await js(`return !!document.querySelector('[aria-label="Cancel"]')`));
	}, 180000);
}

try {
	await step('starts on the welcome screen', async () => {
		await until('the UI', async () => (await text()).includes('Paste a YouTube link') || (await js('return !!document.querySelector("input[aria-label=\'YouTube link\']")')));
		const p = await invoke('platform');
		if (!p.ok?.ffmpeg) throw new Error('ffmpeg not found by the app: ' + JSON.stringify(p));
		await invoke('set_settings', { settings: { folder: DOWNLOADS, preference: 'quality', connections: 8, quality: 'best' } });
		await js('location.reload()');
		await until('reload', async () => (await js('return document.readyState')) === 'complete' && (await text()).includes('dYT'));
		await shot('welcome');
	});

	await step('opens a link as if typed', async () => {
		await typeInto('input[aria-label="YouTube link"]', 'https://www.youtube.com/watch?v=aqz-KE-bpKQ');
		await js(`document.querySelector('input[aria-label="YouTube link"]').form.requestSubmit(); return true`);
		await until('the video', async () => (await text()).includes('Big Buck Bunny'));
		await shot('video');
	});

	await step('clip editor loads playable video and audio', async () => {
		await press('button[aria-label="Preview and pick a clip"]');
		await until('the editor', async () => (await text()).includes('Mark start'));
		const ready = await until('both streams to load', async () => {
			const r = await js(`const v = document.querySelector('video'), a = document.querySelector('audio'); return v && a && v.readyState >= 2 && a.readyState >= 2 ? { v: v.currentSrc.match(/itag=(\\d+)/)?.[1], a: a.currentSrc.match(/itag=(\\d+)/)?.[1] } : null`);
			return r;
		}, 30000);
		console.log(`      preview streams: video itag ${ready.v}, audio itag ${ready.a}`);
		await shot('clip-editor');
	});

	await step(
		'preview plays with audio in sync',
		async () => {
			await press('button[aria-label="Preview and pick a clip"]');
			await until('the editor', async () => (await text()).includes('Mark start'));
			await press('button[aria-label="Play"]');
			const state = await until('playback to advance', async () => {
				const s = await js(`const v = document.querySelector('video'), a = document.querySelector('audio'); return { v: v.currentTime, a: a && a.currentTime }`);
				return s.v > 2 && s.a > 1 ? s : null;
			}, 20000);
			if (Math.abs(state.a - state.v) > 0.5) throw new Error('audio not in sync: ' + JSON.stringify(state));
			console.log(`      video ${state.v.toFixed(2)}s, audio ${state.a.toFixed(2)}s`);
			await clickText('button', 'Pause').catch(() => {});
		},
		// A headless compositor may never hand video frames back; the real desktop does.
		{ soft: true }
	);

	await step('marks a clip: typed start, seek, mark end at the playhead', async () => {
		await press('button[aria-label="Preview and pick a clip"]');
		await until('the editor', async () => (await text()).includes('Mark start'));
		await keys('input[aria-label="Start time"]', '1:00\uE007');
		// Five seconds on with the keyboard (Shift+→), then "Mark end" takes the playhead.
		await wd('POST', S('/actions'), {
			actions: [{ type: 'key', id: 'kb', actions: [{ type: 'keyDown', value: '\uE008' }, { type: 'keyDown', value: '\uE014' }, { type: 'keyUp', value: '\uE014' }, { type: 'keyUp', value: '\uE008' }] }]
		});
		await clickText('button', 'Mark end');
		const [start, end] = await js(`return [document.querySelector('input[aria-label="Start time"]').value, document.querySelector('input[aria-label="End time"]').value]`);
		if (start !== '1:00' || end !== '1:05') throw new Error(`marked ${start} – ${end}, wanted 1:00 – 1:05`);
		await shot('clip-marked');
		await clickText('button', 'Use clip');
		await until('the clip summary', async () => (await text()).includes('1:00 – 1:05'));
	});

	await step('downloads the clip, cut exactly', async () => {
		const before = readdirSync(DOWNLOADS);
		await clickText('button', 'Download ·');
		await waitForJob();
		await shot('clip-done');
		const files = newFiles(before);
		if (files.length !== 1) throw new Error('expected one file, got ' + files);
		const p = probe(files[0]);
		if (p && Math.abs(p.duration - 5) > 0.1) throw new Error(`clip is ${p.duration}s, not 5s`);
		if (p && p.codecs.length !== 2) throw new Error('expected video and audio: ' + p.codecs);
		console.log(`      ${files[0]}: ${p?.duration}s ${p?.codecs}`);
	});

	await step('downloads audio only from a link opened through history', async () => {
		await typeInto('input[aria-label="YouTube link"]', 'jNQXAC9IVRw');
		await js(`document.querySelector('input[aria-label="YouTube link"]').form.requestSubmit(); return true`);
		await until('the zoo video', async () => (await text()).includes('Me at the zoo'));
		await press('button[aria-label="History"]');
		await until('history', async () => js(`return !!document.querySelector('input[aria-label="Search history"]')`));
		await shot('history');
		await clickText('button', '10:35 Big Buck Bunny');
		await until('bunny again', async () => (await text()).includes('Big Buck Bunny') && !(await text()).includes('Search history'));
		await clickText('button', '0:19 Me at the zoo').catch(async () => {
			await typeInto('input[aria-label="YouTube link"]', 'jNQXAC9IVRw');
			await js(`document.querySelector('input[aria-label="YouTube link"]').form.requestSubmit(); return true`);
		});
		await until('the zoo video', async () => (await text()).includes('Me at the zoo'));
		// Video off: audio only.
		await clickText('button', 'Video');
		const before = readdirSync(DOWNLOADS);
		await clickText('button', 'Download ·');
		await waitForJob();
		const files = newFiles(before);
		const p = probe(files[0]);
		if (!/\.(opus|m4a)$/.test(files[0])) throw new Error('not an audio file: ' + files[0]);
		if (p && p.codecs.length !== 1) throw new Error('expected one stream: ' + p.codecs);
		console.log(`      ${files[0]}: ${p?.duration}s ${p?.codecs}`);
		await shot('audio-done');
	});

	await step('raw streams are kept apart', async () => {
		await clickText('button', 'Video');
		await js(`document.querySelector('#raw').click(); return true`);
		const before = readdirSync(DOWNLOADS);
		await clickText('button', 'Download ·');
		await waitForJob();
		const files = newFiles(before);
		if (files.length !== 2) throw new Error('expected two files: ' + files);
		console.log(`      ${files.join(', ')}`);
	});

	await step('bad links get a clear message', async () => {
		await typeInto('input[aria-label="YouTube link"]', 'https://vimeo.com/123');
		await js(`document.querySelector('input[aria-label="YouTube link"]').form.requestSubmit(); return true`);
		await until('the error', async () => (await text()).includes('Not a YouTube link'));
		await shot('bad-link');
	});
} finally {
	await wd('DELETE', S('')).catch(() => {});
}

const failed = results.filter((r) => r[0] === 'FAIL');
console.log(`\n${results.length - failed.length}/${results.length} passed; screenshots in ${OUT}`);
process.exit(failed.length ? 1 : 0);
