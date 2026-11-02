<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Spinner } from '#lib/components/ui/spinner/index.js';
	import LinkIcon from '@lucide/svelte/icons/link';
	import ClipboardIcon from '@lucide/svelte/icons/clipboard-paste';
	import XIcon from '@lucide/svelte/icons/x';
	import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
	import { readText } from '@tauri-apps/plugin-clipboard-manager';
	import { api, inTauri } from '#lib/api.js';
	import { app } from '#lib/state.svelte.js';

	let input: HTMLInputElement | undefined = $state();
	let text = $state(app.link);
	/** A YouTube link found on the clipboard, offered as a one-tap suggestion. */
	let suggestion = $state<string | null>(null);
	let dismissed = '';

	// Follow links opened from elsewhere (history, a second launch, a share).
	$effect(() => {
		text = app.link;
	});

	async function clipboard(): Promise<string> {
		try {
			return (inTauri ? await readText() : await navigator.clipboard.readText())?.trim() ?? '';
		} catch {
			return '';
		}
	}

	async function checkClipboard() {
		const t = await clipboard();
		if (!t || t === dismissed || t === app.link || t.length > 300) return (suggestion = null);
		const parsed = await api.parseLink(t).catch(() => null);
		const current = app.info?.id;
		suggestion = parsed && parsed.videoId !== current ? t : null;
	}

	function submit(value = text) {
		suggestion = null;
		if (value.trim()) app.open(value);
		input?.blur();
	}

	async function paste() {
		const t = await clipboard();
		if (t) {
			text = t;
			submit(t);
		} else {
			input?.focus();
		}
	}

	function onPaste(e: ClipboardEvent) {
		const t = e.clipboardData?.getData('text')?.trim();
		if (!t) return;
		// Pasting a link into an empty box is as good as pressing Enter.
		if (!text.trim() || input?.selectionStart === 0 && input?.selectionEnd === text.length) {
			e.preventDefault();
			text = t;
			submit(t);
		}
	}

	onMount(() => {
		checkClipboard();
		const onFocus = () => checkClipboard();
		window.addEventListener('focus', onFocus);
		return () => window.removeEventListener('focus', onFocus);
	});

	const busy = $derived(app.status === 'loading');
</script>

<!-- novalidate: bare ids and scheme-less links are fine here, the engine parses them. -->
<form
	novalidate
	class="flex flex-col gap-2"
	onsubmit={(e) => {
		e.preventDefault();
		submit();
	}}
>
	<div
		class="flex h-12 items-center gap-1 rounded-xl border bg-card pr-1.5 pl-3.5 shadow-xs transition-[border-color,box-shadow] focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/25 dark:bg-input/30"
	>
		<LinkIcon class="size-4 shrink-0 text-muted-foreground" />
		<input
			bind:this={input}
			bind:value={text}
			onpaste={onPaste}
			type="url"
			inputmode="url"
			enterkeyhint="go"
			autocomplete="off"
			autocapitalize="off"
			spellcheck="false"
			placeholder="Paste a YouTube link"
			aria-label="YouTube link"
			class="h-full min-w-0 flex-1 bg-transparent px-1.5 text-[15px] outline-none placeholder:text-muted-foreground"
		/>
		{#if text}
			<Button variant="ghost" size="icon-sm" aria-label="Clear" onclick={() => { text = ''; input?.focus(); }}>
				<XIcon />
			</Button>
		{:else}
			<Button variant="ghost" size="sm" class="text-muted-foreground" onclick={paste}>
				<ClipboardIcon />Paste
			</Button>
		{/if}
		<Button type="submit" size="icon-sm" class="rounded-lg" disabled={!text.trim() || busy} aria-label="Open link">
			{#if busy}<Spinner />{:else}<ArrowRightIcon />{/if}
		</Button>
	</div>
	{#if suggestion}
		<div class="flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground animate-in fade-in slide-in-from-top-1">
			<ClipboardIcon class="size-4 shrink-0" />
			<button type="button" class="min-w-0 flex-1 truncate text-left" onclick={() => { text = suggestion!; submit(suggestion!); }}>
				Open copied link <span class="opacity-70">{suggestion.replace(/^https?:\/\/(www\.)?/, '')}</span>
			</button>
			<button type="button" class="rounded p-0.5 opacity-70 hover:opacity-100" aria-label="Dismiss" onclick={() => { dismissed = suggestion!; suggestion = null; }}>
				<XIcon class="size-3.5" />
			</button>
		</div>
	{/if}
</form>
