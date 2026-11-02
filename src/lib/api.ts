// The app's backend: typed wrappers over the Tauri commands. In a plain browser (UI work
// and UI tests) a mock stands in, so every screen can be built and checked without Rust.

import { invoke as tauriInvoke, Channel } from '@tauri-apps/api/core';
import { mock } from './mock';

export type StreamKind = 'video' | 'audio' | 'muxed';

export interface Format {
	itag: number;
	kind: StreamKind;
	container: 'mp4' | 'webm' | 'other';
	codec: string;
	codecs: string;
	mime: string;
	width: number | null;
	height: number | null;
	fps: number | null;
	hdr: boolean;
	bitrate: number;
	contentLength: number | null;
	duration: number | null;
	qualityLabel: string | null;
	audioSampleRate: number | null;
	audioChannels: number | null;
	audioTrack: { id: string; name: string; isDefault: boolean } | null;
	drc: boolean;
	url: string;
}

export interface StoryboardLevel {
	url: string;
	width: number;
	height: number;
	count: number;
	cols: number;
	rows: number;
	intervalMs: number;
}

export interface VideoInfo {
	id: string;
	title: string;
	author: string;
	channelId: string;
	duration: number;
	viewCount: number | null;
	isLive: boolean;
	thumbnail: string;
	formats: Format[];
	expiresAt: number;
	client: string;
	storyboard: StoryboardLevel[];
}

export interface DownloadRequest {
	url: string;
	video: number | null;
	audio: number | null;
	clip: [number, number] | null;
	raw: boolean;
	dir: string;
	fileName: string | null;
	ext: string | null;
}

export type Stage = 'preparing' | 'downloading' | 'merging' | 'done';

export interface Progress {
	stage: Stage;
	done: number;
	total: number;
	speed: number;
	eta: number | null;
	fraction: number;
}

export interface EngineError {
	kind: string;
	message: string;
}

export type JobEvent =
	| ({ type: 'progress' } & Progress)
	| { type: 'done'; files: string[]; bytes: number; seconds: number }
	| { type: 'failed'; error: EngineError }
	| { type: 'cancelled' };

export interface Settings {
	folder: string | null;
	preference: 'quality' | 'compatible';
	connections: number;
	quality: string;
}

export interface HistoryEntry {
	videoId: string;
	url: string;
	title: string;
	author: string;
	thumbnail: string;
	duration: number;
	at: number;
	downloads: { files: string[]; clip: [number, number] | null; at: number }[];
}

export interface Folder {
	label: string;
	path: string;
}

export interface Platform {
	os: string;
	ffmpeg: boolean;
	version: string;
}

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	return inTauri ? tauriInvoke<T>(cmd, args) : mock.invoke<T>(cmd, args);
}

/** Errors from the engine arrive as `{kind, message}`; anything else gets wrapped. */
export function asEngineError(e: unknown): EngineError {
	if (e && typeof e === 'object' && 'message' in e) return e as EngineError;
	return { kind: 'unknown', message: String(e) };
}

export const api = {
	resolve: (input: string) => invoke<VideoInfo>('resolve', { input }),
	parseLink: (input: string) => invoke<{ videoId: string; start: number | null } | null>('parse_link', { input }),
	download(request: DownloadRequest, onEvent: (e: JobEvent) => void): Promise<number> {
		if (!inTauri) return mock.download(request, onEvent);
		const channel = new Channel<JobEvent>();
		channel.onmessage = onEvent;
		return tauriInvoke<number>('download', { request, onEvent: channel });
	},
	cancel: (id: number) => invoke<void>('cancel', { id }),
	settings: () => invoke<Settings>('get_settings'),
	setSettings: (settings: Settings) => invoke<void>('set_settings', { settings }),
	history: () => invoke<HistoryEntry[]>('history'),
	historyRemove: (videoId: string) => invoke<HistoryEntry[]>('history_remove', { videoId }),
	historyClear: () => invoke<void>('history_clear'),
	folders: () => invoke<Folder[]>('folders'),
	platform: () => invoke<Platform>('platform'),
	takePendingLink: () => invoke<string | null>('take_pending_link'),
	systemTheme: () => invoke<'dark' | 'light' | null>('system_theme')
};
