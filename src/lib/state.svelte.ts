// App state: the open video and its options, the download queue, history and settings.

import { toast } from 'svelte-sonner';
import { api, asEngineError, type EngineError, type Folder, type HistoryEntry, type JobEvent, type Platform, type Progress, type Settings, type VideoInfo } from './api';
import { audioOptions, outputExt, pickAudio, pickVideo, qualityOptions, sizeOf } from './select';
import { basename } from './format';

export type Status = 'idle' | 'loading' | 'ready' | 'error';

export interface Job {
	/** Stable key in the UI. */
	key: number;
	/** The backend's job id, once it has one. */
	id: number | null;
	videoId: string;
	title: string;
	thumbnail: string;
	summary: string;
	state: 'running' | 'done' | 'failed' | 'cancelled';
	progress: Progress;
	files: string[];
	error: EngineError | null;
	seconds: number;
	bytes: number;
	/** What to send again for "Retry". */
	request: Parameters<typeof api.download>[0];
}

class App {
	// The open video.
	link = $state('');
	status = $state<Status>('idle');
	info = $state<VideoInfo | null>(null);
	error = $state<EngineError | null>(null);
	/** Bumped on every open, so a slow answer for an older link is dropped. */
	#ticket = 0;

	// Options for it.
	wantVideo = $state(true);
	wantAudio = $state(true);
	quality = $state('best');
	audioItag = $state<number | null>(null);
	clip = $state<[number, number] | null>(null);
	raw = $state(false);
	fileName = $state('');

	// Everything else.
	settings = $state<Settings>({ folder: null, preference: 'quality', connections: 8, quality: 'best' });
	folders = $state<Folder[]>([]);
	platform = $state<Platform>({ os: 'linux', ffmpeg: true, version: '' });
	history = $state<HistoryEntry[]>([]);
	jobs = $state<Job[]>([]);

	get folder(): string {
		return this.settings.folder ?? this.folders[0]?.path ?? '';
	}

	get mobile(): boolean {
		return this.platform.os === 'android' || this.platform.os === 'ios';
	}

	get qualities() {
		return this.info ? qualityOptions(this.info, this.settings.preference) : [];
	}

	get audios() {
		return this.info ? audioOptions(this.info) : [];
	}

	get video() {
		if (!this.info || !this.wantVideo) return null;
		return pickVideo(this.info, this.quality === 'best' ? null : Number(this.quality), this.settings.preference);
	}

	get audio() {
		if (!this.info || !this.wantAudio) return null;
		return (this.audioItag != null ? this.info.formats.find((f) => f.itag === this.audioItag) : null) ?? pickAudio(this.info, this.settings.preference);
	}

	get ext() {
		return outputExt(this.video, this.audio, this.clip != null, this.raw);
	}

	get estimate(): number {
		if (!this.info) return 0;
		const d = this.info.duration || 1;
		const total = sizeOf(this.video, d) + sizeOf(this.audio, d);
		// Clips fetch whole segments around the cut; a few seconds' slack each side.
		return this.clip ? (total * Math.min(d, this.clip[1] - this.clip[0] + 6)) / d : total;
	}

	async init() {
		const [settings, folders, platform, history] = await Promise.all([api.settings(), api.folders(), api.platform(), api.history()]);
		this.settings = settings;
		this.folders = folders;
		this.platform = platform;
		this.history = history;
		this.quality = settings.quality || 'best';
		const pending = await api.takePendingLink();
		if (pending) this.open(pending);
	}

	/** Load a link, exactly as if it had been typed and submitted. */
	async open(link: string) {
		link = link.trim();
		if (!link) return;
		this.link = link;
		const ticket = ++this.#ticket;
		this.status = 'loading';
		this.error = null;
		try {
			const info = await api.resolve(link);
			if (ticket !== this.#ticket) return;
			const sameVideo = this.info?.id === info.id;
			this.info = info;
			this.status = 'ready';
			if (!sameVideo) {
				this.clip = null;
				this.fileName = '';
				this.audioItag = null;
			}
			this.history = await api.history();
		} catch (e) {
			if (ticket !== this.#ticket) return;
			this.error = asEngineError(e);
			this.status = 'error';
		}
	}

	close() {
		this.#ticket++;
		this.status = 'idle';
		this.info = null;
		this.error = null;
		this.link = '';
		this.clip = null;
		this.fileName = '';
	}

	async saveSettings(patch: Partial<Settings>) {
		this.settings = { ...this.settings, ...patch };
		await api.setSettings(this.settings);
	}

	async start() {
		const info = this.info;
		if (!info || (!this.video && !this.audio)) return;
		const request = {
			url: info.id,
			video: this.video?.itag ?? null,
			audio: this.audio?.itag ?? null,
			clip: this.clip,
			raw: this.raw,
			dir: this.folder,
			fileName: this.fileName.trim() || null,
			ext: null
		};
		const parts = [this.video ? (this.audio ? 'Video' : 'Video only') : 'Audio only'];
		if (this.video) parts.push(qualityLabel(this));
		if (this.clip) parts.push('Clip');
		if (this.raw) parts.push('Raw');
		await this.#run({ request, videoId: info.id, title: this.fileName.trim() || info.title, thumbnail: info.thumbnail, summary: parts.join(' · ') });
	}

	async retry(job: Job) {
		this.dismiss(job);
		await this.#run({ request: job.request, videoId: job.videoId, title: job.title, thumbnail: job.thumbnail, summary: job.summary });
	}

	async #run(meta: Pick<Job, 'request' | 'videoId' | 'title' | 'thumbnail' | 'summary'>) {
		const key = ++jobKeys;
		this.jobs = [
			{
				...meta,
				key,
				id: null,
				state: 'running',
				progress: { stage: 'preparing', done: 0, total: 0, speed: 0, eta: null, fraction: 0 },
				files: [],
				error: null,
				seconds: 0,
				bytes: 0
			},
			...this.jobs
		];
		const update = (patch: Partial<Job> | ((j: Job) => Partial<Job>)) => {
			this.jobs = this.jobs.map((j) => (j.key === key ? { ...j, ...(typeof patch === 'function' ? patch(j) : patch) } : j));
		};
		const onEvent = (e: JobEvent) => {
			switch (e.type) {
				case 'progress': {
					const { type: _, ...progress } = e;
					update((j) => (j.state === 'running' ? { progress } : {}));
					break;
				}
				case 'done':
					update((j) => ({ state: 'done', files: e.files, seconds: e.seconds, bytes: e.bytes, progress: { ...j.progress, stage: 'done', fraction: 1 } }));
					toast.success('Download finished', { description: e.files.map(basename).join(', ') });
					api.history().then((h) => (this.history = h));
					break;
				case 'failed':
					update({ state: 'failed', error: e.error });
					break;
				case 'cancelled':
					update({ state: 'cancelled' });
					break;
			}
		};
		try {
			const id = await api.download(meta.request, onEvent);
			update({ id });
		} catch (e) {
			update({ state: 'failed', error: asEngineError(e) });
		}
	}

	cancel(job: Job) {
		if (job.id != null) api.cancel(job.id);
	}

	dismiss(job: Job) {
		this.jobs = this.jobs.filter((j) => j.key !== job.key);
	}

	async removeHistory(videoId: string) {
		this.history = await api.historyRemove(videoId);
	}

	async clearHistory() {
		await api.historyClear();
		this.history = [];
	}
}

function qualityLabel(a: App): string {
	const v = a.video;
	if (!v) return '';
	return `${v.height}p`;
}

let jobKeys = 0;

export const app = new App();
