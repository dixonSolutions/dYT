<script lang="ts">
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
</script>

<div class="flex flex-col gap-5" aria-busy="true" aria-label="Loading video">
	<div class="flex gap-4 max-sm:flex-col">
		<Skeleton class="aspect-video w-full rounded-xl sm:w-56" />
		<div class="flex flex-1 flex-col gap-2 py-1">
			<Skeleton class="h-5 w-11/12" />
			<Skeleton class="h-5 w-2/3" />
			<Skeleton class="mt-1 h-4 w-1/3" />
		</div>
	</div>
	<Skeleton class="h-10 w-full" />
	<div class="grid gap-4 sm:grid-cols-2">
		<Skeleton class="h-10" />
		<Skeleton class="h-10" />
	</div>
	<Skeleton class="h-12 w-full rounded-xl" />
</div>
