<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Popover from '#lib/components/ui/popover/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import HistoryIcon from '@lucide/svelte/icons/history';
	import SearchIcon from '@lucide/svelte/icons/search';
	import XIcon from '@lucide/svelte/icons/x';
	import DownloadIcon from '@lucide/svelte/icons/arrow-down-to-line';
	import { app } from '#lib/state.svelte.js';
	import { ago, clock } from '#lib/format.js';

	let open = $state(false);
	let query = $state('');
	let confirmClear = $state(false);

	const items = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return app.history;
		return app.history.filter((h) => h.title.toLowerCase().includes(q) || h.author.toLowerCase().includes(q) || h.videoId.toLowerCase().includes(q));
	});

	function pick(url: string) {
		open = false;
		app.open(url);
	}

	$effect(() => {
		if (!open) {
			query = '';
			confirmClear = false;
		}
	});
</script>

<Popover.Root bind:open>
	<Tooltip.Root>
		<Tooltip.Trigger>
			{#snippet child({ props: tip })}
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button {...tip} {...props} variant="ghost" size="icon" aria-label="History">
							<HistoryIcon class="size-[1.15rem]" />
						</Button>
					{/snippet}
				</Popover.Trigger>
			{/snippet}
		</Tooltip.Trigger>
		<Tooltip.Content>History</Tooltip.Content>
	</Tooltip.Root>
	<Popover.Content align="end" sideOffset={8} class="w-[min(27rem,calc(100vw-1.5rem))] gap-0 overflow-hidden p-0">
		<div class="flex items-center gap-2 border-b px-3 py-2.5">
			<SearchIcon class="size-4 shrink-0 text-muted-foreground" />
			<input
				bind:value={query}
				placeholder="Search history"
				aria-label="Search history"
				class="h-7 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
			/>
		</div>
		<div class="max-h-[min(28rem,60vh)] overflow-y-auto overscroll-contain p-1.5" role="list">
			{#each items as h (h.videoId)}
				<div role="listitem" class="group relative">
					<button
						class="flex w-full items-center gap-3 rounded-lg p-1.5 pr-9 text-left outline-none hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50"
						onclick={() => pick(h.url)}
					>
						<div class="relative aspect-video w-24 shrink-0 overflow-hidden rounded-md bg-muted">
							<img src={h.thumbnail} alt="" loading="lazy" class="size-full object-cover" draggable="false" />
							<span class="tabular absolute right-1 bottom-1 rounded bg-black/75 px-1 text-[10px] font-medium text-white">{clock(h.duration, 0)}</span>
						</div>
						<div class="min-w-0 flex-1">
							<div class="line-clamp-2 text-[13px] leading-snug font-medium">{h.title}</div>
							<div class="mt-0.5 flex items-center gap-1.5 truncate text-xs text-muted-foreground">
								<span class="truncate">{h.author}</span>
								<span aria-hidden="true">·</span>
								<span class="shrink-0">{ago(h.at)}</span>
								{#if h.downloads.length}
									<span class="flex shrink-0 items-center gap-0.5 text-primary" title="Downloaded {h.downloads.length}×">
										<DownloadIcon class="size-3" />{h.downloads.length > 1 ? h.downloads.length : ''}
									</span>
								{/if}
							</div>
						</div>
					</button>
					<Button
						variant="ghost"
						size="icon-xs"
						class="absolute top-1/2 right-1.5 -translate-y-1/2 opacity-100 transition-opacity sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100"
						aria-label="Remove {h.title} from history"
						onclick={() => app.removeHistory(h.videoId)}
					>
						<XIcon />
					</Button>
				</div>
			{:else}
				<div class="px-6 py-10 text-center">
					<HistoryIcon class="mx-auto mb-2 size-6 text-muted-foreground/60" />
					<p class="text-sm font-medium">{query ? 'Nothing matches' : 'No history yet'}</p>
					<p class="mt-1 text-xs text-muted-foreground">{query ? 'Try another word.' : 'Videos you open show up here.'}</p>
				</div>
			{/each}
		</div>
		{#if app.history.length}
			<div class="flex items-center justify-between border-t px-3 py-2">
				<span class="text-xs text-muted-foreground">{app.history.length} {app.history.length === 1 ? 'video' : 'videos'}</span>
				{#if confirmClear}
					<div class="flex gap-1.5">
						<Button size="xs" variant="ghost" onclick={() => (confirmClear = false)}>Keep</Button>
						<Button size="xs" variant="destructive" onclick={() => { app.clearHistory(); confirmClear = false; }}>Clear all</Button>
					</div>
				{:else}
					<Button size="xs" variant="ghost" class="text-muted-foreground" onclick={() => (confirmClear = true)}>Clear history</Button>
				{/if}
			</div>
		{/if}
	</Popover.Content>
</Popover.Root>
