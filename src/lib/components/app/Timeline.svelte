<script lang="ts">
	// The clip timeline: a strip of storyboard frames, the selection between two handles,
	// and the playhead. Drag a handle to move that end (the preview follows), drag anywhere
	// else to scrub. Handles are sliders for the keyboard: arrows move 0.1 s, Shift+arrows 1 s.

	import type { StoryboardLevel } from '#lib/api.js';
	import { clock } from '#lib/format.js';
	import { frameAt } from './storyboard';

	let {
		duration,
		current,
		level,
		start = $bindable(0),
		end = $bindable(0),
		min = 0.1,
		onseek,
		onscrubstart
	}: {
		duration: number;
		current: number;
		level: StoryboardLevel | null;
		start?: number;
		end?: number;
		min?: number;
		onseek: (t: number) => void;
		onscrubstart?: () => void;
	} = $props();

	let track: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let height = $state(0);
	let dragging = $state<'start' | 'end' | 'head' | null>(null);

	const pct = (t: number) => (duration > 0 ? (Math.max(0, Math.min(duration, t)) / duration) * 100 : 0);

	function timeAt(clientX: number): number {
		const r = track!.getBoundingClientRect();
		return Math.max(0, Math.min(duration, ((clientX - r.left) / r.width) * duration));
	}

	function down(kind: 'start' | 'end' | 'head', e: PointerEvent) {
		if (e.button !== 0) return;
		e.preventDefault();
		e.stopPropagation();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		dragging = kind;
		onscrubstart?.();
		move(e);
	}

	function move(e: PointerEvent) {
		if (!dragging) return;
		const t = timeAt(e.clientX);
		if (dragging === 'start') {
			start = Math.min(t, end - min);
			onseek(start);
		} else if (dragging === 'end') {
			end = Math.max(t, start + min);
			onseek(end);
		} else {
			onseek(t);
		}
	}

	function up() {
		dragging = null;
	}

	function key(which: 'start' | 'end', e: KeyboardEvent) {
		const step = e.shiftKey ? 1 : 0.1;
		let d = 0;
		if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') d = -step;
		else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') d = step;
		else if (e.key === 'Home') d = -Infinity;
		else if (e.key === 'End') d = Infinity;
		else return;
		e.preventDefault();
		if (which === 'start') {
			start = Math.max(0, Math.min(start + d, end - min));
			onseek(start);
		} else {
			end = Math.min(duration, Math.max(end + d, start + min));
			onseek(end);
		}
	}

	// Storyboard tiles across the track, each showing the frame at its middle.
	const tiles = $derived.by(() => {
		if (!level || !width || !height || !duration) return [];
		const tileW = (height * level.width) / level.height;
		const n = Math.max(1, Math.ceil(width / tileW));
		return Array.from({ length: n }, (_, k) => ({ w: 100 / n, f: frameAt(level, ((k + 0.5) / n) * duration, duration) }));
	});
</script>

<div class="flex flex-col gap-1.5 select-none">
	<div
		bind:this={track}
		bind:clientWidth={width}
		bind:clientHeight={height}
		class="relative h-14 cursor-pointer touch-none rounded-lg bg-muted sm:h-16"
		role="presentation"
		onpointerdown={(e) => down('head', e)}
		onpointermove={move}
		onpointerup={up}
		onpointercancel={up}
	>
		<!-- Frames -->
		<div class="absolute inset-0 flex overflow-hidden rounded-lg">
			{#each tiles as tile, i (i)}
				<div
					class="h-full shrink-0 bg-no-repeat"
					style="width: {tile.w}%; background-image: url('{tile.f.url}'); background-size: {tile.f.sheetW * 100}% {tile.f.sheetH * 100}%; background-position: {tile.f.x}% {tile.f.y}%;"
				></div>
			{/each}
		</div>
		<!-- Outside the selection is dimmed -->
		<div class="pointer-events-none absolute inset-y-0 left-0 rounded-l-lg bg-background/70" style="width: {pct(start)}%"></div>
		<div class="pointer-events-none absolute inset-y-0 right-0 rounded-r-lg bg-background/70" style="width: {100 - pct(end)}%"></div>
		<!-- Selection frame -->
		<div class="pointer-events-none absolute inset-y-0 border-y-[3px] border-primary" style="left: {pct(start)}%; right: {100 - pct(end)}%"></div>

		<!-- Handles -->
		{#each ['start', 'end'] as const as which (which)}
			{@const t = which === 'start' ? start : end}
			<div
				role="slider"
				tabindex="0"
				aria-label={which === 'start' ? 'Clip start' : 'Clip end'}
				aria-valuemin={0}
				aria-valuemax={duration}
				aria-valuenow={t}
				aria-valuetext={clock(t)}
				class="absolute inset-y-0 z-10 flex w-4 cursor-ew-resize items-center justify-center bg-primary outline-none focus-visible:ring-3 focus-visible:ring-ring/50 {which === 'start' ? '-translate-x-full rounded-l-md' : 'rounded-r-md'} {dragging === which ? 'brightness-110' : ''}"
				style="left: {pct(t)}%"
				onpointerdown={(e) => down(which, e)}
				onkeydown={(e) => key(which, e)}
			>
				<span class="h-5 w-0.5 rounded-full bg-primary-foreground/90"></span>
				<!-- A bigger invisible grab area for fingers -->
				<span class="absolute -inset-x-3 inset-y-0"></span>
			</div>
		{/each}

		<!-- Playhead -->
		<div class="pointer-events-none absolute -inset-y-1.5 z-20 w-0.5 -translate-x-1/2 rounded-full bg-foreground shadow-[0_0_0_1px_var(--background)]" style="left: {pct(current)}%">
			<span class="absolute -top-1 left-1/2 size-2.5 -translate-x-1/2 rounded-full bg-foreground shadow-[0_0_0_1.5px_var(--background)]"></span>
		</div>
	</div>
	<div class="tabular flex justify-between px-0.5 text-[11px] text-muted-foreground">
		<span>0:00</span>
		<span>{clock(duration, 0)}</span>
	</div>
</div>
