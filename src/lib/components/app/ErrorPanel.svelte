<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import AlertIcon from '@lucide/svelte/icons/circle-alert';
	import LockIcon from '@lucide/svelte/icons/lock';
	import RadioIcon from '@lucide/svelte/icons/radio';
	import WifiOffIcon from '@lucide/svelte/icons/wifi-off';
	import LinkIcon from '@lucide/svelte/icons/link-2-off';
	import { app } from '#lib/state.svelte.js';

	const e = $derived(app.error!);
	const view = $derived.by(() => {
		switch (e.kind) {
			case 'invalid_url':
				return { icon: LinkIcon, title: 'Not a YouTube link', body: 'Paste a link to a video, a Short or a youtu.be link.', retry: false };
			case 'age_restricted':
				return { icon: LockIcon, title: 'Age-restricted video', body: "YouTube only shows this one to signed-in accounts, so it can't be downloaded.", retry: false };
			case 'live':
				return { icon: RadioIcon, title: 'Live right now', body: 'Live streams can be downloaded once they have ended.', retry: true };
			case 'network':
				return { icon: WifiOffIcon, title: "Can't reach YouTube", body: 'Check the connection and try again.', retry: true };
			case 'unavailable':
				return { icon: AlertIcon, title: 'Video unavailable', body: e.message.replace(/^This video is unavailable: /, ''), retry: true };
			default:
				return { icon: AlertIcon, title: 'Something went wrong', body: e.message, retry: true };
		}
	});
</script>

<div class="flex flex-col items-center gap-3 rounded-xl border border-dashed px-6 py-10 text-center animate-in fade-in" role="alert">
	<view.icon class="size-7 text-muted-foreground" />
	<div>
		<p class="font-medium">{view.title}</p>
		<p class="selectable mt-1 max-w-sm text-sm text-muted-foreground">{view.body}</p>
	</div>
	{#if view.retry}
		<Button variant="outline" size="sm" onclick={() => app.open(app.link)}>Try again</Button>
	{/if}
</div>
