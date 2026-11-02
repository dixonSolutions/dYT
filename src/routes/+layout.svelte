<script lang="ts">
	import './layout.css';
	import { onMount } from 'svelte';
	import { Toaster } from '#lib/components/ui/sonner/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import { theme } from '#lib/theme.svelte.js';
	import { app } from '#lib/state.svelte.js';

	let { children } = $props();

	onMount(() => theme.start());
</script>

<Tooltip.Provider delayDuration={400}>
	{@render children()}
</Tooltip.Provider>
<!-- Phones: top, clear of the Download button under the thumb. Desktop: out of the way. -->
<Toaster theme={theme.dark ? 'dark' : 'light'} position={app.mobile ? 'top-center' : 'bottom-right'} duration={3500} />
