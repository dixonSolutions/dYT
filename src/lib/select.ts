// Stream choice, mirroring the engine's `select.rs` so the UI can label options with what
// will actually be downloaded (codec, frame rate, size) before anything is fetched.

import type { Format, VideoInfo } from './api';

export type Preference = 'quality' | 'compatible';

function videoRank(codec: string, pref: Preference): number {
	if (pref === 'compatible') return codec === 'avc1' ? 10 : 0;
	return ({ av1: 4, vp9: 3, hevc: 2, avc1: 1 } as Record<string, number>)[codec] ?? 0;
}

function audioRank(codec: string, pref: Preference): number {
	if (pref === 'compatible') return codec === 'mp4a' ? 10 : 0;
	return ({ opus: 3, mp4a: 2, vorbis: 1 } as Record<string, number>)[codec] ?? 0;
}

function compare(a: number[], b: number[]): number {
	for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i];
	return 0;
}

const videoKey = (f: Format, pref: Preference) => [videoRank(f.codec, pref) > 0 ? 1 : 0, f.height ?? 0, f.fps ?? 0, f.hdr ? 0 : 1, videoRank(f.codec, pref), f.bitrate];

export function pickVideo(info: VideoInfo, maxHeight: number | null, pref: Preference): Format | null {
	const cands = info.formats.filter((f) => f.kind === 'video' && f.height);
	const fits = cands.filter((f) => maxHeight == null || (f.height ?? 0) <= maxHeight);
	const best = (list: Format[]) => list.reduce<Format | null>((b, f) => (!b || compare(videoKey(f, pref), videoKey(b, pref)) > 0 ? f : b), null);
	if (fits.length) return best(fits);
	const minH = Math.min(...cands.map((f) => f.height ?? 0));
	return best(cands.filter((f) => f.height === minH));
}

export function pickAudio(info: VideoInfo, pref: Preference): Format | null {
	const key = (f: Format) => [f.audioTrack ? (f.audioTrack.isDefault ? 1 : 0) : 1, f.drc ? 0 : 1, audioRank(f.codec, pref) > 0 ? 1 : 0, Math.floor(f.bitrate / 32000), audioRank(f.codec, pref), f.bitrate];
	return info.formats.filter((f) => f.kind === 'audio').reduce<Format | null>((b, f) => (!b || compare(key(f), key(b)) > 0 ? f : b), null);
}

export interface QualityOption {
	/** `best` or a height. */
	value: string;
	label: string;
	detail: string;
	format: Format;
}

const CODEC_NAMES: Record<string, string> = { av1: 'AV1', vp9: 'VP9', avc1: 'H.264', hevc: 'HEVC', mp4a: 'AAC', opus: 'Opus', vorbis: 'Vorbis' };
export const codecName = (c: string) => CODEC_NAMES[c] ?? c.toUpperCase();

export function heightLabel(f: Format): string {
	const h = f.height ?? 0;
	const fps = f.fps && f.fps > 30 ? String(f.fps) : '';
	const name = h >= 4320 ? '8K' : h >= 2160 ? '4K' : h >= 1440 ? '1440p' : `${h}p`;
	return `${name}${fps && !name.endsWith('K') ? fps : fps ? ` ${fps}` : ''}${f.hdr ? ' HDR' : ''}`;
}

/** One option per height on offer, plus "Best". */
export function qualityOptions(info: VideoInfo, pref: Preference): QualityOption[] {
	const heights = [...new Set(info.formats.filter((f) => f.kind === 'video' && f.height).map((f) => f.height!))].sort((a, b) => b - a);
	const opts: QualityOption[] = [];
	for (const h of heights) {
		const f = pickVideo(info, h, pref);
		if (!f || f.height !== h) continue;
		opts.push({ value: String(h), label: heightLabel(f), detail: codecName(f.codec), format: f });
	}
	const best = pickVideo(info, null, pref);
	if (best) opts.unshift({ value: 'best', label: 'Best', detail: `${heightLabel(best)} · ${codecName(best.codec)}`, format: best });
	return opts;
}

export function audioOptions(info: VideoInfo): Format[] {
	return info.formats
		.filter((f) => f.kind === 'audio' && !f.drc)
		.sort((a, b) => b.bitrate - a.bitrate)
		.filter((f, i, all) => all.findIndex((g) => g.codec === f.codec && Math.abs(g.bitrate - f.bitrate) < 8000 && g.audioTrack?.id === f.audioTrack?.id) === i);
}

export function audioLabel(f: Format): string {
	const kbps = Math.round(f.bitrate / 1000);
	return `${codecName(f.codec)} ${kbps} kbps${f.audioTrack && !f.audioTrack.isDefault ? ` · ${f.audioTrack.name}` : ''}`;
}

/** The extension the engine will give the file (see `select::output_ext`). */
export function outputExt(video: Format | null, audio: Format | null, clip: boolean, raw: boolean): string {
	if (raw) {
		if (video && audio) return `${video.container === 'mp4' ? 'mp4' : video.container} + ${audio.container === 'mp4' ? 'm4a' : audio.container}`;
		const f = video ?? audio;
		return f ? (f.kind === 'audio' && f.container === 'mp4' ? 'm4a' : f.container) : '';
	}
	if (!video && audio) return audio.codec === 'opus' ? 'opus' : audio.codec === 'vorbis' ? 'ogg' : 'm4a';
	if (!video) return '';
	const webmOk = ['vp9', 'av1'].includes(video.codec) && (!audio || ['opus', 'vorbis'].includes(audio.codec));
	if (webmOk && !clip) return 'webm';
	if (['avc1', 'av1', 'vp9', 'hevc'].includes(video.codec) && (!audio || ['mp4a', 'opus', 'ac3'].includes(audio.codec))) return 'mp4';
	return 'mkv';
}

export function sizeOf(f: Format | null, duration: number): number {
	if (!f) return 0;
	return f.contentLength ?? (f.bitrate * duration) / 8;
}
