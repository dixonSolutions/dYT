<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { api, inTauri } from '#lib/api.js';

	// Shown when the window is closed while downloads are still running.
	let open = $state(false);
	let running = $state(0);

	onMount(() => {
		if (!inTauri) return;
		const un = listen<number>('close-requested', (e) => {
			running = e.payload;
			open = true;
		});
		return () => un.then((f) => f());
	});
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
		<Dialog.Header>
			<Dialog.Title>{running === 1 ? 'A download is still running' : `${running} downloads are still running`}</Dialog.Title>
			<Dialog.Description>Quitting now stops {running === 1 ? 'it' : 'them'}; the unfinished files are removed.</Dialog.Description>
		</Dialog.Header>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)}>Keep downloading</Button>
			<Button variant="destructive" onclick={() => api.quit()}>Quit</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
