<script lang="ts">
	import { app } from '#lib/state.svelte.js';
	import { clock } from '#lib/format.js';
	import ScissorsIcon from '@lucide/svelte/icons/scissors';
	import ZapIcon from '@lucide/svelte/icons/zap';
	import MusicIcon from '@lucide/svelte/icons/music-2';

	const recent = $derived(app.history.slice(0, 4));
	const points = [
		{ icon: ZapIcon, title: 'Full speed', body: 'Parallel connections' },
		{ icon: ScissorsIcon, title: 'Exact clips', body: 'Only the part you pick' },
		{ icon: MusicIcon, title: 'Any mix', body: 'Video, audio or both' }
	];
</script>

<div class="flex flex-col gap-8 animate-in fade-in duration-300">
	<div class="grid grid-cols-3 gap-2 text-center text-xs text-muted-foreground sm:gap-3">
		{#each points as p (p.title)}
			<div class="flex flex-col items-center gap-1.5 rounded-xl px-2 py-4">
				<p.icon class="size-5 text-primary" />
				<span class="text-[13px] font-medium text-foreground">{p.title}</span>
				<span>{p.body}</span>
			</div>
		{/each}
	</div>

	{#if recent.length}
		<section class="flex flex-col gap-2">
			<h2 class="text-sm font-medium text-muted-foreground">Recent</h2>
			<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
				{#each recent as h (h.videoId)}
					<button class="group flex flex-col gap-1.5 rounded-lg text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => app.open(h.url)}>
						<div class="relative aspect-video overflow-hidden rounded-lg bg-muted">
							<img src={h.thumbnail} alt="" loading="lazy" class="size-full object-cover transition-transform duration-300 group-hover:scale-[1.04]" draggable="false" />
							<span class="tabular absolute right-1 bottom-1 rounded bg-black/75 px-1 text-[10px] font-medium text-white">{clock(h.duration, 0)}</span>
						</div>
						<span class="line-clamp-2 text-xs leading-snug font-medium">{h.title}</span>
					</button>
				{/each}
			</div>
		</section>
	{/if}
</div>
