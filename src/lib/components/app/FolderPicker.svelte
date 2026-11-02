<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
	import CheckIcon from '@lucide/svelte/icons/check';
	import ChevronIcon from '@lucide/svelte/icons/chevrons-up-down';
	import { open as openDialog } from '@tauri-apps/plugin-dialog';
	import { inTauri } from '#lib/api.js';
	import { app } from '#lib/state.svelte.js';
	import { basename, prettyPath } from '#lib/format.js';

	let { id }: { id?: string } = $props();

	async function choose() {
		if (!inTauri) return;
		const dir = await openDialog({ directory: true, defaultPath: app.folder || undefined, title: 'Save downloads to' });
		if (typeof dir === 'string') app.saveSettings({ folder: dir });
	}

	const custom = $derived(app.folder && !app.folders.some((f) => f.path === app.folder));
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<Button {...props} {id} variant="outline" class="h-10 w-full justify-start gap-2.5 px-3 font-normal" title={app.folder}>
				<FolderIcon class="text-muted-foreground" />
				<span class="min-w-0 flex-1 truncate text-left">{prettyPath(app.folder)}</span>
				<ChevronIcon class="text-muted-foreground" />
			</Button>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="start" class="w-(--bits-dropdown-menu-anchor-width) min-w-56">
		{#each app.folders as f (f.path)}
			<DropdownMenu.Item onSelect={() => app.saveSettings({ folder: f.path })}>
				<FolderIcon />
				<span class="flex-1">{f.label}</span>
				{#if f.path === app.folder}<CheckIcon class="text-primary" />{/if}
			</DropdownMenu.Item>
		{/each}
		{#if custom}
			<DropdownMenu.Item disabled>
				<FolderIcon />
				<span class="flex-1 truncate">{basename(app.folder)}</span>
				<CheckIcon class="text-primary" />
			</DropdownMenu.Item>
		{/if}
		{#if !app.mobile}
			<DropdownMenu.Separator />
			<DropdownMenu.Item onSelect={choose}>
				<FolderOpenIcon />
				Choose folder…
			</DropdownMenu.Item>
		{/if}
	</DropdownMenu.Content>
</DropdownMenu.Root>
