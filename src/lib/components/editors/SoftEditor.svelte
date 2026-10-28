<!-- Text, Open and Shell: one field each, plus the test button. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Action } from '../../api.js';
	import { t } from '../../i18n.js';
	import { studio } from '../../stores/studio.svelte.js';
	import TestButton from './TestButton.svelte';

	let { a }: { a: Extract<Action, { type: 'text' | 'open' | 'shell' }> } = $props();

	let apps = $state<string[]>([]);
	onMount(() => {
		if (a.type === 'open') api.listApps().then((x) => (apps = x)).catch(() => {});
	});
</script>

{#if a.type === 'text'}
	<div>
		<div class="sec-h">{t('text.title')}</div>
		<div class="group">
			<div class="stack">
				<textarea class="field" placeholder={t('text.ph')} value={a.text} oninput={(e) => (a.text = e.currentTarget.value)}></textarea>
			</div>
		</div>
		<div class="footer">{t('text.footer')}</div>
	</div>
{:else if a.type === 'open'}
	<div>
		<div class="sec-h">{t('open.title')}</div>
		<div class="group">
			<div class="stack">
				<input class="field" list="appList" placeholder={t('open.ph')} value={a.target} oninput={(e) => (a.target = e.currentTarget.value)} />
			</div>
		</div>
		<div class="footer">{t('open.footer')}</div>
		<datalist id="appList">
			{#each apps as x (x)}<option value={x}></option>{/each}
		</datalist>
	</div>
{:else}
	<div>
		<div class="sec-h">{t('shell.title')}</div>
		<div class="group">
			<div class="stack">
				<textarea class="field mono" value={a.command} oninput={(e) => (a.command = e.currentTarget.value)}></textarea>
			</div>
		</div>
		<div class="footer">{@html t('shell.footer', { shell: studio.catalog?.shell ?? 'sh -lc' })}</div>
	</div>
{/if}
<TestButton {a} />
