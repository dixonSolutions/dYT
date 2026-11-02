<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Slider } from '#lib/components/ui/slider/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import CodeIcon from '@lucide/svelte/icons/code-xml';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import { openUrl } from '@tauri-apps/plugin-opener';
	import FolderPicker from './FolderPicker.svelte';
	import { app } from '#lib/state.svelte.js';
	import { inTauri } from '#lib/api.js';

	let { open = $bindable(false) }: { open?: boolean } = $props();

	const QUALITIES = [
		['best', 'Best available'],
		['2160', '4K (2160p)'],
		['1440', '1440p'],
		['1080', '1080p'],
		['720', '720p'],
		['480', '480p'],
		['360', '360p']
	] as const;

	let connections = $state([8]);
	$effect(() => {
		if (open) connections = [app.settings.connections];
	});

	function setQuality(q: string) {
		app.saveSettings({ quality: q });
		app.quality = q;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[calc(100dvh-2rem)] gap-0 overflow-y-auto p-0 sm:max-w-md">
		<Dialog.Header class="px-6 pt-6 pb-2">
			<Dialog.Title>Settings</Dialog.Title>
		</Dialog.Header>

		<div class="flex flex-col gap-6 px-6 py-4">
			<section class="flex flex-col gap-2">
				<Label for="settings-folder">Save downloads to</Label>
				<FolderPicker id="settings-folder" />
			</section>

			<section class="flex flex-col gap-2">
				<span class="text-sm font-medium">Formats</span>
				<div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label="Formats">
					{#each [['quality', 'Best quality', 'AV1 or VP9 with Opus'], ['compatible', 'Most compatible', 'MP4: H.264 with AAC']] as [value, title, detail] (value)}
						<button
							role="radio"
							aria-checked={app.settings.preference === value}
							onclick={() => app.saveSettings({ preference: value as 'quality' | 'compatible' })}
							class="rounded-lg border p-3 text-left transition-colors outline-none hover:bg-muted/60 focus-visible:ring-3 focus-visible:ring-ring/50 aria-checked:border-primary aria-checked:bg-accent"
						>
							<div class="text-sm font-medium">{title}</div>
							<div class="mt-0.5 text-xs text-muted-foreground">{detail}</div>
						</button>
					{/each}
				</div>
			</section>

			<section class="flex flex-col gap-2">
				<Label for="settings-quality">Default video quality</Label>
				<Select.Root type="single" value={app.settings.quality} onValueChange={setQuality}>
					<Select.Trigger id="settings-quality" class="h-10 w-full">
						{QUALITIES.find(([v]) => v === app.settings.quality)?.[1] ?? 'Best available'}
					</Select.Trigger>
					<Select.Content>
						{#each QUALITIES as [value, label] (value)}
							<Select.Item {value} {label}>{label}</Select.Item>
						{/each}
					</Select.Content>
				</Select.Root>
				<p class="text-xs text-muted-foreground">The best stream at or below this height is picked for every video.</p>
			</section>

			<section class="flex flex-col gap-3">
				<div class="flex items-center justify-between">
					<span id="settings-connections" class="text-sm font-medium">Parallel connections</span>
					<span class="tabular text-sm text-muted-foreground">{connections[0]}</span>
				</div>
				<Slider
					type="multiple"
					bind:value={connections}
					min={1}
					max={16}
					step={1}
					aria-labelledby="settings-connections"
					onValueCommit={(v: number[]) => app.saveSettings({ connections: v[0] })}
				/>
				<p class="text-xs text-muted-foreground">More connections are faster on fast lines. 8 suits most.</p>
			</section>
		</div>

		<div class="flex items-center justify-between gap-3 border-t px-6 py-4 text-xs text-muted-foreground">
			<div class="flex flex-col gap-1">
				<span>dYT {app.platform.version}</span>
				{#if app.platform.ffmpeg}
					<span class="flex items-center gap-1"><CircleCheckIcon class="size-3.5 text-emerald-600 dark:text-emerald-400" />Merging and clips ready</span>
				{:else}
					<span class="flex items-center gap-1 text-amber-600 dark:text-amber-400"><TriangleAlertIcon class="size-3.5" />ffmpeg missing: single streams only</span>
				{/if}
			</div>
			<button
				class="flex items-center gap-1.5 rounded-md px-2 py-1 hover:bg-muted hover:text-foreground"
				onclick={() => (inTauri ? openUrl('https://github.com/dixonSolutions/dYT') : window.open('https://github.com/dixonSolutions/dYT'))}
			>
				<CodeIcon class="size-3.5" />Source
			</button>
		</div>
	</Dialog.Content>
</Dialog.Root>
