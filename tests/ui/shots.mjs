// Screenshots of every screen against the mock backend, light and dark, desktop and phone.
//   node tests/ui/shots.mjs [outdir]   (with `pnpm dev` running)
import { chromium } from 'playwright';

const out = process.argv[2] ?? '.scratch/shots';
const base = process.env.DYT_URL ?? 'http://localhost:5173/';
const browser = await chromium.launch();

for (const scheme of ['light', 'dark']) {
	for (const [name, viewport] of [['desktop', { width: 980, height: 760 }], ['phone', { width: 390, height: 844 }]]) {
		const page = await browser.newPage({ viewport, colorScheme: scheme, deviceScaleFactor: name === 'phone' ? 2 : 1 });
		const errors = [];
		page.on('pageerror', (e) => errors.push(e.message));
		page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
		const shot = (s) => page.screenshot({ path: `${out}/${scheme}-${name}-${s}.png` });
		await page.goto(base);
		await page.waitForTimeout(600);
		await shot('1-welcome');
		await page.getByLabel('YouTube link').fill('https://youtu.be/dQw4w9WgXcQ');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(250);
		await shot('2-loading');
		await page.waitForTimeout(900);
		await shot('3-video');
		await page.getByRole('button', { name: 'Preview and pick a clip' }).click();
		await page.waitForTimeout(500);
		await shot('4-clip');
		await page.getByLabel('Start time').fill('0:30');
		await page.getByLabel('End time').fill('1:12.5');
		await page.getByLabel('End time').press('Enter');
		await shot('4b-clip-set');
		await page.getByRole('button', { name: 'Use clip' }).click();
		await page.waitForTimeout(400);
		await shot('5-with-clip');
		await page.getByRole('button', { name: /^Download ·/ }).click();
		await page.waitForTimeout(1200);
		await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
		await shot('6-downloading');
		await page.waitForTimeout(4500);
		await shot('7-done');
		await page.evaluate(() => window.scrollTo(0, 0));
		await page.getByRole('button', { name: 'History' }).click();
		await page.waitForTimeout(400);
		await shot('8-history');
		await page.keyboard.press('Escape');
		await page.getByRole('button', { name: 'Settings' }).click();
		await page.waitForTimeout(400);
		await shot('9-settings');
		await page.keyboard.press('Escape');
		await page.getByLabel('YouTube link').fill('aaaaaaaaaaa');
		await page.keyboard.press('Enter');
		await page.waitForTimeout(1000);
		await shot('10-error');
		if (errors.length) console.log(scheme, name, 'errors:', errors);
		await page.close();
	}
}
await browser.close();
console.log('ok');
