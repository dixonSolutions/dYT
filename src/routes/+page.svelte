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

	onMount(() => {
		app.init();
		if (!inTauri) return;
		// Links from a second launch (`dyt <link>`) or shared to the app.
		const un = listen<string>('open-link', (e) => app.open(e.payload));
		return () => un.then((f) => f());
	});
</script>

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
