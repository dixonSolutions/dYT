<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import TopBar from '#lib/components/app/TopBar.svelte';
	import LinkInput from '#lib/components/app/LinkInput.svelte';
	import VideoPanel from '#lib/components/app/VideoPanel.svelte';
	import LoadingCard from '#lib/components/app/LoadingCard.svelte';
	import ErrorPanel from '#lib/components/app/ErrorPanel.svelte';
	import Welcome from '#lib/components/app/Welcome.svelte';
	import DownloadList from '#lib/components/app/DownloadList.svelte';
	import { inTauri } from '#lib/api.js';
	import { app } from '#lib/state.svelte.js';

	let dropping = $state(false);

	/** A YouTube link in dropped or pasted text, if there is one. */
	function linkIn(text: string | undefined | null): string | null {
		const t = (text ?? '').trim();
		const m = t.match(/https?:\/\/\S+/);
		const candidate = m ? m[0] : t;
		return /(youtube\.com|youtu\.be|youtube-nocookie\.com)\//i.test(candidate) || /^[\w-]{11}$/.test(candidate) ? candidate : null;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		dropping = false;
		const link = linkIn(e.dataTransfer?.getData('text/uri-list')?.split('\n').find((l) => !l.startsWith('#')) || e.dataTransfer?.getData('text/plain'));
		if (link) app.open(link);
	}

	// Ctrl+V anywhere outside a text field opens a copied link.
	function onPaste(e: ClipboardEvent) {
		const t = e.target;
		if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || (t instanceof HTMLElement && t.isContentEditable)) return;
		const link = linkIn(e.clipboardData?.getData('text'));
		if (link) {
			e.preventDefault();
			app.open(link);
		}
	}

	onMount(() => {
		app.init();
		if (!inTauri) return;
		// Links from a second launch (`dyt <link>`) or shared to the app.
		const un = listen<string>('open-link', (e) => app.open(e.payload));
		return () => un.then((f) => f());
	});
</script>

<svelte:window
	onpaste={onPaste}
	ondragover={(e) => {
		if (e.dataTransfer?.types.some((t) => t === 'text/uri-list' || t === 'text/plain')) {
			e.preventDefault();
			dropping = true;
		}
	}}
	ondragleave={(e) => {
		if (!e.relatedTarget) dropping = false;
	}}
	ondrop={onDrop}
/>

{#if dropping}
	<div class="pointer-events-none fixed inset-3 z-50 grid place-items-center rounded-2xl border-2 border-dashed border-primary bg-background/80 backdrop-blur-sm animate-in fade-in">
		<p class="text-lg font-medium text-primary">Drop the link to open it</p>
	</div>
{/if}

<div class="flex min-h-dvh flex-col">
	<TopBar />
	<main class="safe-bottom mx-auto flex w-full max-w-3xl flex-1 flex-col gap-6 px-4 pt-5 pb-10">
		<LinkInput />
		{#if app.status === 'loading'}
			<LoadingCard />
		{:else if app.status === 'error'}
			<ErrorPanel />
		{:else if app.status === 'ready' && app.info}
			<VideoPanel />
		{/if}
		<DownloadList />
		{#if app.status === 'idle'}
			<Welcome />
		{/if}
	</main>
</div>
