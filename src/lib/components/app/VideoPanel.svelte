<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as ToggleGroup from '#lib/components/ui/toggle-group/index.js';
	import { Switch } from '#lib/components/ui/switch/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import FilmIcon from '@lucide/svelte/icons/film';
	import MusicIcon from '@lucide/svelte/icons/music-2';
	import ScissorsIcon from '@lucide/svelte/icons/scissors';
	import PlayIcon from '@lucide/svelte/icons/play';
	import XIcon from '@lucide/svelte/icons/x';
	import DownloadIcon from '@lucide/svelte/icons/arrow-down-to-line';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import FolderPicker from './FolderPicker.svelte';
	import ClipEditor from './ClipEditor.svelte';
	import { app } from '#lib/state.svelte.js';
	import { audioLabel, codecName } from '#lib/select.js';
	import { bytes, clock, views } from '#lib/format.js';

	const info = $derived(app.info!);
	let editorOpen = $state(false);

	// The two stream toggles; at least one stays on.
	const streams = $derived([app.wantVideo && 'video', app.wantAudio && 'audio'].filter(Boolean) as string[]);
	function setStreams(v: string[]) {
		if (!v.length) return;
		app.wantVideo = v.includes('video');
		app.wantAudio = v.includes('audio');
	}

	const quality = $derived(app.qualities.find((q) => q.value === app.quality) ?? app.qualities[0]);
	const bestAudio = $derived(app.audio);

	function setRange(v: string) {
		if (v === 'clip') editorOpen = true;
		else if (v === 'full') app.clip = null;
	}

	const placeholder = $derived(info.title);
	const canDownload = $derived(!!(app.video || app.audio) && !!app.folder && !app.blocker);
</script>

<div class="flex flex-col gap-5 animate-in fade-in slide-in-from-bottom-1 duration-200">
	<!-- What it is -->
	<div class="flex gap-4 max-sm:flex-col">
		<button
			class="group relative aspect-video w-full shrink-0 overflow-hidden rounded-xl bg-muted outline-none focus-visible:ring-3 focus-visible:ring-ring/50 sm:w-56"
			onclick={() => (editorOpen = true)}
			aria-label="Preview and pick a clip"
		>
			<img src={info.thumbnail} alt="" class="size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]" draggable="false" />
			<span class="tabular absolute right-2 bottom-2 rounded-md bg-black/75 px-1.5 py-0.5 text-xs font-medium text-white">{clock(info.duration, 0)}</span>
			<span class="absolute inset-0 grid place-items-center bg-black/0 transition-colors group-hover:bg-black/25">
				<span class="grid size-11 scale-90 place-items-center rounded-full bg-black/55 text-white opacity-0 backdrop-blur transition-all group-hover:scale-100 group-hover:opacity-100 group-focus-visible:opacity-100">
					<PlayIcon class="ml-0.5 size-5 fill-current" />
				</span>
			</span>
		</button>
		<div class="flex min-w-0 flex-col gap-1.5 py-0.5">
			<h1 class="selectable line-clamp-3 text-lg leading-snug font-semibold tracking-tight">{info.title}</h1>
			<p class="truncate text-sm text-muted-foreground">
				{info.author}{#if info.viewCount != null}<span class="mx-1.5" aria-hidden="true">·</span>{views(info.viewCount)}{/if}
			</p>
		</div>
	</div>

	<!-- What to get -->
	<div class="grid gap-x-4 gap-y-4 sm:grid-cols-2">
		<div class="flex flex-col gap-2 sm:col-span-2">
			<span class="text-sm font-medium">Download</span>
			<ToggleGroup.Root type="multiple" variant="outline" value={streams} onValueChange={setStreams} class="w-full" aria-label="Streams">
				<ToggleGroup.Item value="video" class="h-10 flex-1 gap-2 data-[state=on]:border-primary/50 data-[state=on]:bg-accent data-[state=on]:text-accent-foreground">
					<FilmIcon />Video
				</ToggleGroup.Item>
				<ToggleGroup.Item value="audio" class="h-10 flex-1 gap-2 data-[state=on]:border-primary/50 data-[state=on]:bg-accent data-[state=on]:text-accent-foreground">
					<MusicIcon />Audio
				</ToggleGroup.Item>
			</ToggleGroup.Root>
		</div>

		{#if app.wantVideo}
			<div class="flex flex-col gap-2">
				<Label for="quality">Video quality</Label>
				<Select.Root type="single" value={app.quality} onValueChange={(v) => (app.quality = v)}>
					<Select.Trigger id="quality" class="h-10 w-full">
						<span class="flex min-w-0 items-baseline gap-1.5 truncate">
							{quality?.label}
							<span class="truncate text-xs text-muted-foreground">{quality?.detail}</span>
						</span>
					</Select.Trigger>
					<Select.Content class="max-h-80">
						{#each app.qualities as q (q.value)}
							<Select.Item value={q.value} label={q.label}>
								<span class="flex w-full items-baseline justify-between gap-4">
									<span>{q.label} <span class="text-xs text-muted-foreground">{q.detail}</span></span>
									<span class="tabular text-xs text-muted-foreground">{bytes(q.format.contentLength)}</span>
								</span>
							</Select.Item>
						{/each}
					</Select.Content>
				</Select.Root>
			</div>
		{/if}

		{#if app.wantAudio}
			<div class="flex flex-col gap-2">
				<Label for="audio">Audio</Label>
				<Select.Root type="single" value={String(app.audioItag ?? 'best')} onValueChange={(v) => (app.audioItag = v === 'best' ? null : Number(v))}>
					<Select.Trigger id="audio" class="h-10 w-full">
						<span class="truncate">{app.audioItag == null ? 'Best' : ''} {#if bestAudio}<span class={app.audioItag == null ? 'text-xs text-muted-foreground' : ''}>{audioLabel(bestAudio)}</span>{/if}</span>
					</Select.Trigger>
					<Select.Content>
						<Select.Item value="best" label="Best">Best</Select.Item>
						{#each app.audios as a (a.itag)}
							<Select.Item value={String(a.itag)} label={audioLabel(a)}>
								<span class="flex w-full justify-between gap-4">
									<span>{audioLabel(a)}</span>
									<span class="tabular text-xs text-muted-foreground">{bytes(a.contentLength)}</span>
								</span>
							</Select.Item>
						{/each}
					</Select.Content>
				</Select.Root>
			</div>
		{/if}

		<div class="flex flex-col gap-2 sm:col-span-2">
			<span class="text-sm font-medium">Length</span>
			{#if app.clip}
				<div class="flex h-10 items-center gap-2 rounded-lg border border-primary/40 bg-accent pr-1 pl-3 text-accent-foreground">
					<ScissorsIcon class="size-4 shrink-0" />
					<button class="tabular min-w-0 flex-1 truncate text-left text-sm font-medium" onclick={() => (editorOpen = true)}>
						{clock(app.clip[0])} – {clock(app.clip[1])}
						<span class="font-normal opacity-75">· {clock(app.clip[1] - app.clip[0])}</span>
					</button>
					<Button variant="ghost" size="icon-sm" class="hover:bg-background/60" aria-label="Edit clip" onclick={() => (editorOpen = true)}><PencilIcon /></Button>
					<Button variant="ghost" size="icon-sm" class="hover:bg-background/60" aria-label="Download the whole video instead" onclick={() => (app.clip = null)}><XIcon /></Button>
				</div>
			{:else}
				<ToggleGroup.Root type="single" variant="outline" value="full" onValueChange={setRange} class="w-full" aria-label="Length">
					<ToggleGroup.Item value="full" class="h-10 flex-1 data-[state=on]:border-primary/50 data-[state=on]:bg-accent data-[state=on]:text-accent-foreground">Whole video</ToggleGroup.Item>
					<ToggleGroup.Item value="clip" class="h-10 flex-1 gap-2"><ScissorsIcon />Clip…</ToggleGroup.Item>
				</ToggleGroup.Root>
			{/if}
		</div>

		<div class="flex flex-col gap-2">
			<Label for="folder">Save to</Label>
			<FolderPicker id="folder" />
		</div>

		<div class="flex flex-col gap-2">
			<Label for="filename">File name</Label>
			<div class="flex h-10 items-center rounded-lg border bg-transparent pr-3 shadow-xs transition-[border-color,box-shadow] focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50 dark:bg-input/30">
				<input
					id="filename"
					bind:value={app.fileName}
					{placeholder}
					autocomplete="off"
					spellcheck="false"
					class="h-full min-w-0 flex-1 bg-transparent px-3 text-sm outline-none placeholder:text-foreground/80"
				/>
				<span class="shrink-0 text-sm text-muted-foreground">.{app.ext}</span>
			</div>
		</div>

		<label class="flex items-start gap-3 rounded-lg sm:col-span-2" for="raw">
			<Switch id="raw" bind:checked={app.raw} class="mt-0.5" />
			<span class="flex flex-col gap-0.5">
				<span class="text-sm font-medium">Raw streams</span>
				<span class="text-xs text-muted-foreground">Keep YouTube's original files as they are, without merging video and audio.</span>
			</span>
		</label>
	</div>

	<Button size="lg" class="h-12 w-full rounded-xl text-[15px] shadow-sm shadow-primary/20" disabled={!canDownload} onclick={() => app.start()}>
		<DownloadIcon class="size-[1.15rem]" />
		Download
		{#if app.estimate > 0}<span class="tabular font-normal opacity-80">· {app.clip ? '≈ ' : ''}{bytes(app.estimate)}</span>{/if}
	</Button>
	{#if app.blocker}
		<p class="-mt-2 text-center text-xs text-destructive" role="status">{app.blocker}</p>
	{:else if app.video && !app.raw}
		<p class="-mt-2 text-center text-xs text-muted-foreground">
			{`${app.video.height}p ${codecName(app.video.codec)}${app.audio ? ` + ${codecName(app.audio.codec)}` : ''} → .${app.ext}`}
		</p>
	{/if}
</div>

<ClipEditor bind:open={editorOpen} />
