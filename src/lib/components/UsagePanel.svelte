<script lang="ts">
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { kindOf } from '../vocab.js';

	const rows = $derived(
		(studio.mapping?.layers ?? []).map((l) =>
			studio.names.map((n) => {
				const a = l.slots[n];
				return a ? kindOf(a.type) : '';
			}),
		),
	);
	const dev = $derived(rows.flat().filter((k) => k === 'dev').length);
	const agt = $derived(rows.flat().filter((k) => k === 'agt').length);
	const total = $derived(studio.names.length * rows.length);
</script>

<div>
	<div class="sec-h">{t('usage')}</div>
	<div class="group">
		<div class="stack">
			<div class="usage"><b>{dev + agt}</b><span>{t('usage.of', { n: total })}</span></div>
			<div class="cells">
				{#each rows as r, i (i)}
					<div class="r" style:grid-template-columns="repeat({studio.names.length},1fr)">
						{#each r as c, j (j)}<i class={c}></i>{/each}
					</div>
				{/each}
			</div>
		</div>
		<div class="row"><span class="cat dev"></span><span class="k">{t('usage.dev')}</span><span class="v">{dev}</span></div>
		<div class="row">
			<span class="cat agt"></span><span class="k">{t('usage.agt')}</span>
			<span class="v">{agt} / {studio.catalog?.software_slots ?? 0}</span>
		</div>
	</div>
</div>
