import { describe, expect, it } from 'vitest';
import type { Format, VideoInfo } from './api';
import { audioOptions, outputExt, pickAudio, pickVideo, qualityOptions } from './select';

function f(itag: number, kind: 'video' | 'audio', codec: string, height: number | null, fps: number | null, bitrate: number): Format {
	const container = ['avc1', 'mp4a', 'av1'].includes(codec) ? 'mp4' : 'webm';
	return { itag, kind, container, codec, codecs: codec, mime: `${kind}/${container}`, width: null, height, fps, hdr: false, bitrate, contentLength: bitrate, duration: 10, qualityLabel: null, audioSampleRate: null, audioChannels: null, audioTrack: null, drc: false, url: '' };
}

const info: VideoInfo = {
	id: 'x', title: 't', author: '', channelId: '', duration: 10, viewCount: null, isLive: false, thumbnail: '', expiresAt: 0, client: '', storyboard: [],
	formats: [
		f(137, 'video', 'avc1', 1080, 30, 3_000_000),
		f(248, 'video', 'vp9', 1080, 30, 1_200_000),
		f(399, 'video', 'av1', 1080, 30, 1_100_000),
		f(313, 'video', 'vp9', 2160, 30, 13_000_000),
		f(136, 'video', 'avc1', 720, 30, 1_000_000),
		f(298, 'video', 'avc1', 720, 60, 1_500_000),
		f(140, 'audio', 'mp4a', null, null, 130_000),
		f(251, 'audio', 'opus', null, null, 129_000),
		f(250, 'audio', 'opus', null, null, 61_000)
	]
};

// Same expectations as the engine's select.rs tests: the UI must label what the engine picks.
describe('stream choice matches the engine', () => {
	it('picks video', () => {
		expect(pickVideo(info, null, 'quality')?.itag).toBe(313);
		expect(pickVideo(info, 1080, 'quality')?.itag).toBe(399);
		expect(pickVideo(info, 1080, 'compatible')?.itag).toBe(137);
		expect(pickVideo(info, 720, 'quality')?.itag).toBe(298);
		expect(pickVideo(info, 144, 'quality')?.itag).toBe(298);
	});
	it('picks audio', () => {
		expect(pickAudio(info, 'quality')?.itag).toBe(251);
		expect(pickAudio(info, 'compatible')?.itag).toBe(140);
	});
	it('names the container', () => {
		const g = (i: number) => info.formats.find((x) => x.itag === i)!;
		expect(outputExt(g(313), g(251), false, false)).toBe('webm');
		expect(outputExt(g(313), g(251), true, false)).toBe('mp4');
		expect(outputExt(g(137), g(140), false, false)).toBe('mp4');
		expect(outputExt(null, g(251), false, false)).toBe('opus');
		expect(outputExt(null, g(140), false, false)).toBe('m4a');
		expect(outputExt(g(137), g(140), false, true)).toBe('mp4 + m4a');
	});
	it('offers one quality per height plus Best', () => {
		const q = qualityOptions(info, 'quality');
		expect(q.map((o) => o.value)).toEqual(['best', '2160', '1080', '720']);
		expect(q[0].format.itag).toBe(313);
		expect(audioOptions(info).map((a) => a.itag)).toEqual([140, 251, 250]);
	});
});
