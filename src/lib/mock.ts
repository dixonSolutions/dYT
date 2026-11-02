// A stand-in backend for running the UI in a plain browser (development and UI tests).
// It behaves like the real one: latency, progress events, errors for bad links.

import type { DownloadRequest, Format, HistoryEntry, JobEvent, Settings, VideoInfo } from './api';

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

function fmt(itag: number, kind: 'video' | 'audio', codec: string, height: number | null, fps: number | null, mb: number, container: 'mp4' | 'webm'): Format {
	return {
		itag,
		kind,
		container,
		codec,
		codecs: codec,
		mime: `${kind}/${container}`,
		width: height ? Math.round((height * 16) / 9) : null,
		height,
		fps,
		hdr: false,
		bitrate: Math.round((mb * 8e6) / 213),
		contentLength: Math.round(mb * 1e6),
		duration: 213,
		qualityLabel: height ? `${height}p` : null,
		audioSampleRate: kind === 'audio' ? 48000 : null,
		audioChannels: kind === 'audio' ? 2 : null,
		audioTrack: null,
		drc: false,
		url: ''
	};
}

const SAMPLE: VideoInfo = {
	id: 'dQw4w9WgXcQ',
	title: 'Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)',
	author: 'Rick Astley',
	channelId: 'UCuAXFkgsw1L7xaCfnd5JJOw',
	duration: 213,
	viewCount: 1832934452,
	isLive: false,
	thumbnail: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg',
	formats: [
		fmt(313, 'video', 'vp9', 2160, 25, 358.6, 'webm'),
		fmt(401, 'video', 'av1', 2160, 25, 240.3, 'mp4'),
		fmt(400, 'video', 'av1', 1440, 25, 122.2, 'mp4'),
		fmt(137, 'video', 'avc1', 1080, 25, 80.9, 'mp4'),
		fmt(399, 'video', 'av1', 1080, 25, 30.4, 'mp4'),
		fmt(136, 'video', 'avc1', 720, 25, 26.5, 'mp4'),
		fmt(398, 'video', 'av1', 720, 25, 17.6, 'mp4'),
		fmt(135, 'video', 'avc1', 480, 25, 14.1, 'mp4'),
		fmt(134, 'video', 'avc1', 360, 25, 8.4, 'mp4'),
		fmt(133, 'video', 'avc1', 240, 25, 4.3, 'mp4'),
		fmt(160, 'video', 'avc1', 144, 25, 2.1, 'mp4'),
		fmt(140, 'audio', 'mp4a', null, null, 3.4, 'mp4'),
		fmt(251, 'audio', 'opus', null, null, 3.4, 'webm'),
		fmt(250, 'audio', 'opus', null, null, 1.6, 'webm')
	],
	expiresAt: Date.now() / 1000 + 20000,
	client: 'mock',
	storyboard: []
};

let settings: Settings = { folder: null, preference: 'quality', connections: 8, quality: 'best' };
let history: HistoryEntry[] = [
	{
		videoId: 'aqz-KE-bpKQ',
		url: 'https://www.youtube.com/watch?v=aqz-KE-bpKQ',
		title: 'Big Buck Bunny 60fps 4K - Official Blender Foundation Short Film',
		author: 'Blender',
		thumbnail: 'https://i.ytimg.com/vi/aqz-KE-bpKQ/maxresdefault.jpg',
		duration: 635,
		at: Date.now() - 3600_000 * 5,
		downloads: [{ files: ['/home/me/Downloads/bunny clip.mp4'], clip: [300, 312.5], at: Date.now() - 3600_000 * 5 }]
	},
	{
		videoId: 'jNQXAC9IVRw',
		url: 'https://www.youtube.com/watch?v=jNQXAC9IVRw',
		title: 'Me at the zoo',
		author: 'jawed',
		thumbnail: 'https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg',
		duration: 19,
		at: Date.now() - 86400_000 * 2,
		downloads: []
	}
];

const ID = /(?:v=|youtu\.be\/|shorts\/|embed\/|live\/)([\w-]{11})|^([\w-]{11})$/;

export const mock = {
	async invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
		switch (cmd) {
			case 'resolve': {
				await delay(700);
				const m = String(args.input).trim().match(ID);
				if (!m) throw { kind: 'invalid_url', message: "That doesn't look like a YouTube link or video id" };
				const id = m[1] ?? m[2];
				if (id === 'aaaaaaaaaaa') throw { kind: 'unavailable', message: 'This video is unavailable: This video is unavailable' };
				const known = history.find((h) => h.videoId === id);
				const info = known ? { ...SAMPLE, id, title: known.title, author: known.author, thumbnail: known.thumbnail, duration: known.duration } : { ...SAMPLE, id };
				history = [{ videoId: id, url: `https://www.youtube.com/watch?v=${id}`, title: info.title, author: info.author, thumbnail: info.thumbnail, duration: info.duration, at: Date.now(), downloads: known?.downloads ?? [] }, ...history.filter((h) => h.videoId !== id)];
				return info as T;
			}
			case 'parse_link': {
				const m = String(args.input).trim().match(ID);
				return (m ? { videoId: m[1] ?? m[2], start: null } : null) as T;
			}
			case 'get_settings':
				return { ...settings } as T;
			case 'set_settings':
				settings = args.settings as Settings;
				return undefined as T;
			case 'history':
				return [...history] as T;
			case 'history_remove':
				history = history.filter((h) => h.videoId !== args.videoId);
				return [...history] as T;
			case 'history_clear':
				history = [];
				return undefined as T;
			case 'folders':
				return [
					{ label: 'Downloads', path: '/home/me/Downloads' },
					{ label: 'Videos', path: '/home/me/Videos' },
					{ label: 'Music', path: '/home/me/Music' }
				] as T;
			case 'platform':
				return { os: 'linux', ffmpeg: true, version: '0.1.0' } as T;
			case 'take_pending_link':
			case 'system_theme':
				return null as T;
			case 'cancel':
				cancelled.add(args.id as number);
				return undefined as T;
		}
		throw { kind: 'unknown', message: `mock: no command ${cmd}` };
	},

	async download(req: DownloadRequest, onEvent: (e: JobEvent) => void): Promise<number> {
		const id = ++jobs;
		const total = req.clip ? 6e6 : 42e6;
		(async () => {
			onEvent({ type: 'progress', stage: 'preparing', done: 0, total: 0, speed: 0, eta: null, fraction: 0 });
			await delay(400);
			for (let done = 0; done <= total; done += total / 30) {
				if (cancelled.has(id)) return onEvent({ type: 'cancelled' });
				onEvent({ type: 'progress', stage: 'downloading', done, total, speed: 11.4e6, eta: (total - done) / 11.4e6, fraction: (done / total) * 0.97 });
				await delay(120);
			}
			onEvent({ type: 'progress', stage: 'merging', done: total, total, speed: 0, eta: null, fraction: 0.99 });
			await delay(500);
			const ext = req.video ? 'mp4' : 'opus';
			const name = `${req.dir}/${req.fileName ?? 'video'}.${ext}`;
			onEvent({ type: 'done', files: [name], bytes: total, seconds: 4.2 });
		})();
		return id;
	}
};

let jobs = 0;
const cancelled = new Set<number>();
