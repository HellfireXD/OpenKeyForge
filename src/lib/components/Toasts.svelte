<script lang="ts">
	import { toasts } from '../stores/toasts.svelte.js';
	import { icon } from '../vocab.js';

	const TILE = { ok: ['t-open', 'check'], err: ['t-media', 'warn'], info: ['t-mouse', 'info'] } as const;
</script>

<div class="toasts">
	{#each toasts.items as x (x.id)}
		<div class="toast" style:opacity={x.leaving ? 0 : 1} style:transition="opacity .3s">
			<span class="tile {TILE[x.kind][0]}">{@html icon(TILE[x.kind][1])}</span>
			<div>
				<b>{x.title}</b>
				{#if x.sub}<span style="color:var(--label-2)">{x.sub}</span>{/if}
			</div>
		</div>
	{/each}
</div>
