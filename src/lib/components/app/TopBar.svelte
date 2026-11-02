<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import SettingsIcon from '@lucide/svelte/icons/settings-2';
	import HistoryPopover from './HistoryPopover.svelte';
	import SettingsDialog from './SettingsDialog.svelte';
	import { app } from '#lib/state.svelte.js';

	let settingsOpen = $state(false);
</script>

<header class="safe-top sticky top-0 z-30 border-b border-border/60 bg-background/85 backdrop-blur-md">
	<div class="mx-auto flex h-14 max-w-3xl items-center gap-2 px-4">
		<button class="flex items-center gap-2.5 rounded-lg outline-none focus-visible:ring-3 focus-visible:ring-ring/50" onclick={() => app.close()} aria-label="dYT home">
			<img src="/icon.svg" alt="" class="size-7" draggable="false" />
			<span class="text-[1.05rem] font-semibold tracking-tight">dYT</span>
		</button>
		<div class="flex-1"></div>
		<HistoryPopover />
		<Tooltip.Root>
			<Tooltip.Trigger>
				{#snippet child({ props })}
					<Button {...props} variant="ghost" size="icon" aria-label="Settings" onclick={() => (settingsOpen = true)}>
						<SettingsIcon class="size-[1.15rem]" />
					</Button>
				{/snippet}
			</Tooltip.Trigger>
			<Tooltip.Content>Settings</Tooltip.Content>
		</Tooltip.Root>
	</div>
</header>

<SettingsDialog bind:open={settingsOpen} />
