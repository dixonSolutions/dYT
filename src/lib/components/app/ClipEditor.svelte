<script lang="ts">
	import { untrack } from 'svelte';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Spinner } from '#lib/components/ui/spinner/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import PlayIcon from '@lucide/svelte/icons/play';
	import PauseIcon from '@lucide/svelte/icons/pause';
	import SquareIcon from '@lucide/svelte/icons/square';
	import RewindIcon from '@lucide/svelte/icons/rotate-ccw';
	import ForwardIcon from '@lucide/svelte/icons/rotate-cw';
	import RepeatIcon from '@lucide/svelte/icons/repeat';
	import type { Format, StoryboardLevel } from '#lib/api.js';
	import { app } from '#lib/state.svelte.js';
	import { clock, parseClock } from '#lib/format.js';
	import Timeline from './Timeline.svelte';
	import { frameAt, pickLevel } from './storyboard';

	let { open = $bindable(false) }: { open?: boolean } = $props();

	const MIN = 0.1;
	const info = $derived(app.info!);
	const duration = $derived(info?.duration || 0);

	let start = $state(0);
	let end = $state(0);
	let current = $state(0);
	let playing = $state(false);
	let buffering = $state(false);
	/** Playing the selection once, from start to end. */
	let previewing = $state(false);
	let videoFailed = $state(false);
	let audioFailed = $state(false);

	let video: HTMLVideoElement | undefined = $state();
	let audio: HTMLAudioElement | undefined = $state();

	// Low-resolution streams for the preview, in the order worth trying: what this webview says
	// it can decode first (WebKitGTK without an H.264 decoder can still play VP9 and AV1).
	const probe = typeof document !== 'undefined' ? document.createElement('video') : null;
	function canPlay(f: Format): number {
		const answer = probe?.canPlayType(`${f.mime.split(';')[0]}; codecs="${f.codecs}"`) ?? '';
		return answer === 'probably' ? 2 : answer === 'maybe' ? 1 : 0;
	}
	function videoCandidates(formats: Format[]): Format[] {
		const rank = (f: Format) => canPlay(f) * 100000 + ({ avc1: 3, vp9: 2, av1: 1 }[f.codec] ?? 0) * 10000 - Math.abs((f.height ?? 0) - 480);
		return formats.filter((f) => f.kind === 'video' && f.height && f.url && (f.height ?? 0) <= 720 && canPlay(f) > 0).sort((a, b) => rank(b) - rank(a));
	}
	function audioCandidates(formats: Format[]): Format[] {
		const rank = (f: Format) => canPlay(f) * 100000 + (f.codec === 'mp4a' ? 2 : f.codec === 'opus' ? 1 : 0) * 10000 + Math.min(f.bitrate, 160000) / 100;
		return formats.filter((f) => f.kind === 'audio' && f.url && !f.drc && (!f.audioTrack || f.audioTrack.isDefault) && canPlay(f) > 0).sort((a, b) => rank(b) - rank(a));
	}
	const videos = $derived(info ? videoCandidates(info.formats) : []);
	const audios = $derived(info ? audioCandidates(info.formats) : []);
	/** Which candidate is playing; an error moves on to the next one. */
	let vIndex = $state(0);
	let aIndex = $state(0);
	const vsrc = $derived(videos[vIndex]?.url);
	const asrc = $derived(audios[aIndex]?.url);
	$effect(() => {
		if (vIndex >= videos.length && videos.length) videoFailed = true;
	});
	const level = $derived<StoryboardLevel | null>(info ? pickLevel(info.storyboard, 320) : null);
	const showStill = $derived(!vsrc || videoFailed);
	/** The element whose clock drives everything: the video, or the audio without one. */
	const master = $derived(showStill ? (audioFailed ? null : audio) : video);

	// Each time the editor opens, start from the current clip (or the whole video).
	$effect(() => {
		const isOpen = open;
		untrack(() => {
			if (isOpen && info) {
				[start, end] = app.clip ?? [0, duration];
				current = start;
				videoFailed = false;
				audioFailed = false;
				vIndex = 0;
				aIndex = 0;
				previewing = false;
			}
			if (!isOpen) pause();
		});
	});

	// Keep the playhead moving smoothly (timeupdate fires only ~4×/s).
	$effect(() => {
		if (!playing) return;
		let raf = 0;
		const tick = () => {
			const m = master;
			if (m) {
				current = m.currentTime;
				if (previewing && current >= end) {
					pause();
					seek(end);
					previewing = false;
				}
				// Keep the audio within a frame or two of the video.
				if (m === video && audio && !audioFailed && !audio.paused && Math.abs(audio.currentTime - video!.currentTime) > 0.2) {
					audio.currentTime = video!.currentTime;
				}
			}
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(raf);
	});

	async function play() {
		const m = master;
		if (!m) return;
		if (current >= duration - 0.05) seek(0);
		// Start from where the playhead shows, even if an earlier seek didn't take.
		if (Math.abs(m.currentTime - current) > 0.25) m.currentTime = current;
		try {
			if (m === video && audio && !audioFailed) {
				audio.currentTime = video!.currentTime;
				audio.play().catch(() => {});
			}
			await m.play();
			playing = true;
		} catch {
			playing = false;
		}
	}

	function pause() {
		video?.pause();
		audio?.pause();
		playing = false;
	}

	let lastSeek = 0;
	let pendingSeek: number | null = null;
	/** Seek both elements; called continuously while dragging, so it's throttled. */
	function seek(t: number) {
		t = Math.max(0, Math.min(duration, t));
		current = t;
		const now = performance.now();
		const apply = (to: number) => {
			// Exact seeks only: fastSeek lands on keyframes, and WebKitGTK sometimes drops it.
			for (const el of [video, audio]) if (el) el.currentTime = to;
			lastSeek = performance.now();
		};
		if (now - lastSeek > 70) apply(t);
		else {
			if (pendingSeek == null) setTimeout(() => { if (pendingSeek != null) apply(pendingSeek); pendingSeek = null; }, 70);
			pendingSeek = t;
		}
	}

	function toggle() {
		previewing = false;
		if (playing) pause();
		else play();
	}

	function markStart() {
		start = Math.min(current, end - MIN);
		if (start < 0) start = 0;
	}

	/** While playing this is "stop here": pause and close the clip at this moment. */
	function markEnd() {
		// Playing: the media clock. Paused: the playhead as shown (a seek may still be settling).
		const t = playing ? (master?.currentTime ?? current) : current;
		if (playing) pause();
		if (t <= start + MIN) {
			// Marking an end before the start moves the start back instead of failing.
			start = Math.max(0, t - MIN);
		}
		end = Math.max(t, start + MIN);
		current = end;
		previewing = false;
	}

	function previewClip() {
		previewing = true;
		seek(start);
		play();
	}

	function nudge(by: number) {
		seek(current + by);
	}

	function done() {
		const whole = start <= 0.05 && end >= duration - 0.05;
		app.clip = whole ? null : [Math.round(start * 1000) / 1000, Math.round(end * 1000) / 1000];
		open = false;
	}

	// Editable timestamps: committed on Enter or blur, reverted when not a time.
	let startText = $state('');
	let endText = $state('');
	let editingStart = false;
	let editingEnd = false;
	$effect(() => {
		if (!editingStart) startText = clock(start);
	});
	$effect(() => {
		if (!editingEnd) endText = clock(end);
	});
	function commitStart() {
		editingStart = false;
		const t = parseClock(startText);
		if (t != null) {
			start = Math.max(0, Math.min(t, end - MIN));
			seek(start);
		}
		startText = clock(start);
	}
	function commitEnd() {
		editingEnd = false;
		const t = parseClock(endText);
		if (t != null) {
			end = Math.min(duration, Math.max(t, start + MIN));
			seek(end);
		}
		endText = clock(end);
	}
	function timeKeys(e: KeyboardEvent, which: 'start' | 'end') {
		if (e.key === 'Enter') {
			// Commit right away; blur alone isn't delivered while the window lacks focus.
			if (which === 'start') commitStart();
			else commitEnd();
			(e.currentTarget as HTMLInputElement).blur();
		}
		if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
			e.preventDefault();
			const step = (e.shiftKey ? 1 : 0.1) * (e.key === 'ArrowUp' ? 1 : -1);
			if (which === 'start') start = Math.max(0, Math.min(start + step, end - MIN));
			else end = Math.min(duration, Math.max(end + step, start + MIN));
			seek(which === 'start' ? start : end);
		}
	}

	function onKey(e: KeyboardEvent) {
		if (!open) return;
		const t = e.target;
		// Typing a time, or nudging a handle with the arrows, isn't a shortcut.
		if (t instanceof HTMLElement && (t.tagName === 'INPUT' || t.getAttribute('role') === 'slider' || t.isContentEditable)) return;
		const k = e.key;
		if (k === ' ' || k === 'k') toggle();
		else if (k === 'i' || k === '[') markStart();
		else if (k === 'o' || k === ']') markEnd();
		else if (k === 'ArrowLeft') nudge(e.shiftKey ? -5 : -1);
		else if (k === 'ArrowRight') nudge(e.shiftKey ? 5 : 1);
		else if (k === ',') nudge(-1 / 30);
		else if (k === '.') nudge(1 / 30);
		else if (k === 'p') previewClip();
		else return;
		e.preventDefault();
	}

	const still = $derived(level ? frameAt(level, current, duration) : null);
	const length = $derived(Math.max(0, end - start));
</script>

<svelte:window onkeydown={onKey} />

<Dialog.Root bind:open>
	<Dialog.Content
		class="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0 max-sm:inset-0 max-sm:h-dvh max-sm:max-h-dvh max-sm:max-w-full max-sm:translate-x-0 max-sm:translate-y-0 max-sm:rounded-none max-sm:ring-0 sm:max-w-3xl"
		onOpenAutoFocus={(e) => e.preventDefault()}
	>
		<Dialog.Header class="safe-top flex-row items-baseline gap-3 px-5 pt-4 pb-3 sm:px-6">
			<Dialog.Title>Clip</Dialog.Title>
			<Dialog.Description class="tabular truncate">{clock(length)} of {clock(duration, 0)}</Dialog.Description>
		</Dialog.Header>

		<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 pb-4 sm:px-6">
			<!-- Preview -->
			<div class="relative aspect-video w-full overflow-hidden rounded-xl bg-black">
				{#if open && vsrc && !videoFailed}
					<!-- svelte-ignore a11y_media_has_caption -->
					<video
						bind:this={video}
						src={vsrc}
						class="size-full"
						playsinline
						muted
						preload="auto"
						disablepictureinpicture
						onloadedmetadata={() => video && (video.currentTime = current)}
						onwaiting={() => { buffering = true; audio?.pause(); }}
						onplaying={() => { buffering = false; if (playing && audio && !audioFailed && audio.paused) { audio.currentTime = video!.currentTime; audio.play().catch(() => {}); } }}
						oncanplay={() => (buffering = false)}
						onended={() => pause()}
						onerror={() => (vIndex + 1 < videos.length ? vIndex++ : (videoFailed = true))}
						onclick={toggle}
					></video>
				{/if}
				{#if showStill}
					{#if still}
						<div
							class="size-full bg-no-repeat"
							style="background-image: url('{still.url}'); background-size: {still.sheetW * 100}% {still.sheetH * 100}%; background-position: {still.x}% {still.y}%;"
							role="img"
							aria-label="Preview frame"
						></div>
					{:else}
						<img src={info?.thumbnail} alt="" class="size-full object-cover opacity-80" />
					{/if}
				{/if}
				{#if open && asrc}
					<audio bind:this={audio} src={asrc} preload="auto" onerror={() => (aIndex + 1 < audios.length ? aIndex++ : (audioFailed = true))} onended={() => showStill && pause()}></audio>
				{/if}
				{#if buffering && playing}
					<div class="absolute inset-0 grid place-items-center"><Spinner class="size-8 text-white" /></div>
				{:else if !playing}
					<button class="group absolute inset-0 grid place-items-center" onclick={toggle} aria-label="Play">
						<span class="grid size-14 place-items-center rounded-full bg-black/55 text-white backdrop-blur transition-transform group-hover:scale-105">
							<PlayIcon class="ml-1 size-6 fill-current" />
						</span>
					</button>
				{/if}
				<span class="tabular pointer-events-none absolute bottom-2 left-2 rounded-md bg-black/65 px-1.5 py-0.5 text-xs font-medium text-white">{clock(current)}</span>
			</div>

			<!-- Timeline -->
			<Timeline
				{duration}
				{current}
				{level}
				bind:start
				bind:end
				min={MIN}
				onseek={(t) => seek(t)}
				onscrubstart={() => { if (playing) pause(); previewing = false; }}
			/>

			<!-- Transport -->
			<div class="flex items-center justify-center gap-1.5 sm:gap-2">
				<Button variant="outline" class="h-10 gap-1.5 px-3 max-sm:px-2.5" onclick={markStart}>
					<span class="font-mono text-base leading-none font-semibold" aria-hidden="true">[</span>Mark start
				</Button>
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button {...props} variant="ghost" size="icon-lg" aria-label="Back 5 seconds" onclick={() => nudge(-5)}><RewindIcon /></Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Back 5 s <span class="opacity-60">(⇧←)</span></Tooltip.Content>
				</Tooltip.Root>
				<Button variant="secondary" size="icon-lg" class="size-12 rounded-full" aria-label={playing ? 'Pause' : 'Play'} onclick={toggle}>
					{#if playing}<PauseIcon class="size-5 fill-current" />{:else}<PlayIcon class="ml-0.5 size-5 fill-current" />{/if}
				</Button>
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button {...props} variant="ghost" size="icon-lg" aria-label="Forward 5 seconds" onclick={() => nudge(5)}><ForwardIcon /></Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Forward 5 s <span class="opacity-60">(⇧→)</span></Tooltip.Content>
				</Tooltip.Root>
				<Button variant={playing ? 'default' : 'outline'} class="h-10 gap-1.5 px-3" onclick={markEnd}>
					{#if playing}<SquareIcon class="size-3.5 fill-current" />Stop here{:else}Mark end<span class="font-mono text-base leading-none font-semibold" aria-hidden="true">]</span>{/if}
				</Button>
			</div>

			<!-- Exact times -->
			<div class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-end gap-2 sm:gap-3">
				<label class="flex min-w-0 flex-col gap-1.5">
					<span class="text-xs font-medium text-muted-foreground">Start</span>
					<input
						class="tabular h-10 w-full min-w-0 rounded-lg border bg-transparent px-3 text-sm shadow-xs outline-none focus:border-ring focus:ring-3 focus:ring-ring/50 dark:bg-input/30"
						bind:value={startText}
						onfocus={() => (editingStart = true)}
						onblur={commitStart}
						onkeydown={(e) => timeKeys(e, 'start')}
						inputmode="decimal"
						aria-label="Start time"
					/>
				</label>
				<label class="flex min-w-0 flex-col gap-1.5">
					<span class="text-xs font-medium text-muted-foreground">End</span>
					<input
						class="tabular h-10 w-full min-w-0 rounded-lg border bg-transparent px-3 text-sm shadow-xs outline-none focus:border-ring focus:ring-3 focus:ring-ring/50 dark:bg-input/30"
						bind:value={endText}
						onfocus={() => (editingEnd = true)}
						onblur={commitEnd}
						onkeydown={(e) => timeKeys(e, 'end')}
						inputmode="decimal"
						aria-label="End time"
					/>
				</label>
				<Button variant="outline" class="h-10 gap-1.5 max-sm:w-10 max-sm:px-0" onclick={previewClip} disabled={!master} aria-label="Preview the clip">
					<RepeatIcon /><span class="max-sm:hidden">Preview</span>
				</Button>
			</div>
			<p class="hidden text-center text-xs text-muted-foreground sm:block">
				<kbd class="rounded border px-1 font-sans">Space</kbd> play · <kbd class="rounded border px-1 font-sans">I</kbd> mark start ·
				<kbd class="rounded border px-1 font-sans">O</kbd> mark end · <kbd class="rounded border px-1 font-sans">←</kbd><kbd class="rounded border px-1 font-sans">→</kbd> seek ·
				<kbd class="rounded border px-1 font-sans">,</kbd><kbd class="rounded border px-1 font-sans">.</kbd> frame
			</p>
		</div>

		<div class="safe-bottom flex items-center justify-end gap-2 border-t px-5 py-3 sm:px-6">
			<Button variant="ghost" onclick={() => { start = 0; end = duration; seek(0); }}>Reset</Button>
			<div class="flex-1"></div>
			<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
			<Button onclick={done} disabled={length < MIN}>Use clip</Button>
		</div>
	</Dialog.Content>
</Dialog.Root>
