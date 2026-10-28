<script lang="ts">
	import type { Action } from '../../api.js';
	import { t } from '../../i18n.js';
	import { studio } from '../../stores/studio.svelte.js';
	import { icon, MEDIA_ICON, mediaName } from '../../vocab.js';

	let { a }: { a: Extract<Action, { type: 'media' }> } = $props();

	const known = $derived(studio.catalog?.media ?? []);
	const custom = $derived(typeof a.code === 'number' || !known.includes(a.code));
</script>

<div>
	<div class="sec-h">{t('media.title')}</div>
	<div class="group">
		<div class="stack">
			<div class="tiles">
				{#each known as c (c)}
					<button aria-pressed={a.code === c} onclick={() => (a.code = c)}>
						{@html icon(...(MEDIA_ICON[c] || ['media']))}{mediaName(c)}
					</button>
				{/each}
			</div>
		</div>
		<div class="row">
			<span class="k">{t('media.custom')}</span>
			<input
				class="field mono"
				placeholder="0x1B6"
				value={custom ? String(a.code) : ''}
				oninput={(e) => {
					const v = e.currentTarget.value.trim();
					if (v) a.code = v;
				}}
			/>
		</div>
	</div>
	<div class="footer">{t('media.footer')}</div>
</div>
