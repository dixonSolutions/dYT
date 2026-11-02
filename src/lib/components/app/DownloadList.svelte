<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import { Progress } from '#lib/components/ui/progress/index.js';
	import XIcon from '@lucide/svelte/icons/x';
	import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
	import PlayIcon from '@lucide/svelte/icons/play';
	import RotateIcon from '@lucide/svelte/icons/rotate-ccw';
	import CheckIcon from '@lucide/svelte/icons/circle-check';
	import AlertIcon from '@lucide/svelte/icons/circle-alert';
	import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
	import { inTauri } from '#lib/api.js';
	import { app, type Job } from '#lib/state.svelte.js';
	import { basename, bytes, eta, speed } from '#lib/format.js';

	function status(j: Job): string {
		const p = j.progress;
		switch (j.state) {
			case 'done':
				return `${bytes(j.bytes)} in ${j.seconds < 10 ? j.seconds.toFixed(1) : Math.round(j.seconds)} s`;
			case 'failed':
				return j.error?.message ?? 'Failed';
			case 'cancelled':
				return 'Cancelled';
		}
		if (p.stage === 'preparing') return 'Starting…';
		if (p.stage === 'merging') return 'Finishing…';
		return [`${bytes(p.done)} of ${bytes(p.total)}`, speed(p.speed), eta(p.eta)].filter(Boolean).join(' · ');
	}

	const open = (f: string) => inTauri && openPath(f).catch(() => {});
	const reveal = (f: string) => inTauri && revealItemInDir(f).catch(() => {});
</script>

{#if app.jobs.length}
	<section class="flex flex-col gap-2" aria-label="Downloads">
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-medium text-muted-foreground">Downloads</h2>
			{#if app.jobs.some((j) => j.state !== 'running')}
				<Button variant="ghost" size="xs" class="text-muted-foreground" onclick={() => (app.jobs = app.jobs.filter((j) => j.state === 'running'))}>Clear finished</Button>
			{/if}
		</div>
		<ul class="flex flex-col gap-2">
			{#each app.jobs as j (j.key)}
				<li class="flex items-center gap-3 rounded-xl border bg-card p-2.5 pr-2 animate-in fade-in slide-in-from-top-1 duration-200">
					<div class="relative aspect-video w-20 shrink-0 overflow-hidden rounded-md bg-muted">
						<img src={j.thumbnail} alt="" class="size-full object-cover" draggable="false" />
						{#if j.state === 'done'}
							<span class="absolute inset-0 grid place-items-center bg-black/35"><CheckIcon class="size-5 text-white" /></span>
						{:else if j.state === 'failed'}
							<span class="absolute inset-0 grid place-items-center bg-black/45"><AlertIcon class="size-5 text-white" /></span>
						{/if}
					</div>
					<div class="flex min-w-0 flex-1 flex-col gap-1">
						<div class="flex items-baseline gap-2">
							<span class="truncate text-sm font-medium">{j.state === 'done' && j.files.length ? basename(j.files[0]) : j.title}</span>
						</div>
						{#if j.state === 'running'}
							<Progress value={j.progress.fraction * 100} class="h-1" />
						{/if}
						<div class="tabular flex min-w-0 items-center gap-1.5 text-xs {j.state === 'failed' ? 'text-destructive' : 'text-muted-foreground'}">
							<span class="shrink-0">{j.summary}</span>
							<span aria-hidden="true">·</span>
							<span class="truncate" title={status(j)}>{status(j)}</span>
						</div>
					</div>
					<div class="flex shrink-0 items-center">
						{#if j.state === 'running'}
							<Button variant="ghost" size="icon-sm" aria-label="Cancel" onclick={() => app.cancel(j)}><XIcon /></Button>
						{:else}
							{#if j.state === 'done' && j.files[0]}
								<Button variant="ghost" size="icon-sm" aria-label="Open" title="Open" onclick={() => open(j.files[0])}><PlayIcon /></Button>
								{#if !app.mobile}
									<Button variant="ghost" size="icon-sm" aria-label="Show in folder" title="Show in folder" onclick={() => reveal(j.files[0])}><FolderOpenIcon /></Button>
								{/if}
							{:else}
								<Button variant="ghost" size="icon-sm" aria-label="Retry" title="Retry" onclick={() => app.retry(j)}><RotateIcon /></Button>
							{/if}
							<Button variant="ghost" size="icon-sm" class="text-muted-foreground" aria-label="Dismiss" onclick={() => app.dismiss(j)}><XIcon /></Button>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	</section>
{/if}
