<script lang="ts">
	import './layout.css';
	import { onMount } from 'svelte';
	import { Toaster } from '#lib/components/ui/sonner/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import { theme } from '#lib/theme.svelte.js';

	let { children } = $props();

	onMount(() => theme.start());
</script>

<Tooltip.Provider delayDuration={400}>
	{@render children()}
</Tooltip.Provider>
<Toaster theme={theme.dark ? 'dark' : 'light'} position="bottom-center" offset={{ bottom: 'calc(env(safe-area-inset-bottom) + 16px)' }} />
