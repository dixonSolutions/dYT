import { describe, expect, it } from 'vitest';
import { bytes, clock, eta, parseClock, prettyPath } from './format';

describe('clock', () => {
	it('formats short and long times', () => {
		expect(clock(0)).toBe('0:00');
		expect(clock(61.5)).toBe('1:01.5');
		expect(clock(3723.25)).toBe('1:02:03.25');
		expect(clock(59.999)).toBe('1:00');
		expect(clock(125.5, 0)).toBe('2:06');
		expect(clock(-3)).toBe('0:00');
	});
	it('round-trips through parseClock', () => {
		for (const t of [0, 1.5, 59.25, 61, 3599.5, 3723.25]) expect(parseClock(clock(t))).toBeCloseTo(t, 2);
	});
});

describe('parseClock', () => {
	it('reads every way people type a time', () => {
		expect(parseClock('75')).toBe(75);
		expect(parseClock('75.5')).toBe(75.5);
		expect(parseClock('1:15')).toBe(75);
		expect(parseClock('1:02:03.5')).toBe(3723.5);
		expect(parseClock('1m15s')).toBe(75);
		expect(parseClock('2h')).toBe(7200);
		expect(parseClock(' 0:30 ')).toBe(30);
	});
	it('rejects what is not a time', () => {
		for (const s of ['', 'abc', '1:75', '1::2', '-5', '1:2:3:4']) expect(parseClock(s)).toBeNull();
	});
});

describe('sizes and paths', () => {
	it('formats sizes', () => {
		expect(bytes(512)).toBe('512 B');
		expect(bytes(3_449_447)).toBe('3.4 MB');
		expect(bytes(358_608_461)).toBe('359 MB');
		expect(bytes(2.5e9)).toBe('2.50 GB');
		expect(bytes(null)).toBe('');
	});
	it('shortens paths', () => {
		expect(prettyPath('/home/me/Downloads')).toBe('~/Downloads');
		expect(prettyPath('/var/home/me/Videos')).toBe('~/Videos');
		expect(prettyPath('/storage/emulated/0/Download')).toBe('Phone/Download');
	});
	it('says how long is left', () => {
		expect(eta(0.5)).toBe('a moment left');
		expect(eta(42.2)).toBe('43 s left');
		expect(eta(600)).toBe('10 min left');
		expect(eta(null)).toBe('');
	});
});
