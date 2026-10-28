<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Action, type MacroStep } from '../../api.js';
	import { t } from '../../i18n.js';
	import { recorder } from '../../recorder.js';
	import { num } from '../../vocab.js';
	import TestButton from './TestButton.svelte';

	let { a }: { a: Extract<Action, { type: 'macro' }> } = $props();

	type Kind = MacroStep['do'];
	const KINDS: Kind[] = ['keys', 'text', 'wait', 'open', 'shell'];
	const fresh = (k: Kind): MacroStep => (k === 'wait' ? { do: 'wait', ms: 300 } : { do: k, value: '' });

	let apps = $state<string[]>([]);
	onMount(() => {
		api.listApps().then((x) => (apps = x)).catch(() => {});
	});

	function move(i: number, d: number) {
		const j = i + d;
		if (j < 0 || j >= a.steps.length) return;
		[a.steps[i], a.steps[j]] = [a.steps[j], a.steps[i]];
	}

	const placeholder = (k: Kind) =>
		({ keys: t('macro.keysPh'), text: t('macro.textPh'), shell: t('macro.shellPh'), open: t('open.ph'), wait: '' })[k];
</script>

<div>
	<div class="sec-h">{t('macro.title')}</div>
	<div class="group">
		<div>
			{#if !a.steps.length}
				<div class="row" style="color:var(--label-2)">{t('macro.first')}</div>
			{/if}
			{#each a.steps as st, i (i)}
				<div class="step">
					<span class="no">{i + 1}</span>
					<div class="step-h">
						<select value={st.do} onchange={(e) => (a.steps[i] = fresh(e.currentTarget.value as Kind))}>
							{#each KINDS as k (k)}<option value={k}>{t('step.' + k)}</option>{/each}
						</select>
						<span class="sp"></span>
						{#if st.do === 'keys'}
							<button
								class="btn rec"
								use:recorder={(acc) => {
									if (st.do === 'keys') st.value = st.value ? `${st.value}, ${acc}` : acc;
								}}><span class="rd"></span><span>{t('keys.rec')}</span></button
							>
						{/if}
						<button class="ib" title={t('macro.up')} onclick={() => move(i, -1)}>↑</button>
						<button class="ib" title={t('macro.down')} onclick={() => move(i, 1)}>↓</button>
						<button class="ib" title={t('macro.del')} onclick={() => a.steps.splice(i, 1)}>✕</button>
					</div>
					{#if st.do === 'wait'}
						<div class="range">
							<input type="range" min="0" max="5000" step="50" value={num(st.ms)} oninput={(e) => (st.ms = Math.max(0, num(e.currentTarget.value)))} />
							<input class="field num" type="number" min="0" value={num(st.ms)} oninput={(e) => (st.ms = Math.max(0, num(e.currentTarget.value)))} />
							<span class="unit">ms</span>
						</div>
					{:else}
						{#if st.do === 'text' || st.do === 'shell'}
							<textarea
								class="field"
								class:mono={st.do === 'shell'}
								style="min-height:56px"
								placeholder={placeholder(st.do)}
								value={st.value}
								oninput={(e) => {
									st.value = e.currentTarget.value;
								}}
							></textarea>
						{:else}
							<input
								class="field"
								class:mono={st.do === 'keys'}
								list={st.do === 'open' ? 'appList2' : undefined}
								placeholder={placeholder(st.do)}
								value={st.value}
								oninput={(e) => {
									st.value = e.currentTarget.value;
								}}
							/>
						{/if}
						<div class="rep">
							{t('macro.repeat')}
							<input
								class="field num"
								style="width:56px!important"
								type="number"
								min="1"
								max="999"
								value={Math.max(1, num(st.repeat, 1))}
								oninput={(e) => {
									st.repeat = Math.max(1, num(e.currentTarget.value, 1));
								}}
							/>
							{t('macro.times')}
						</div>
					{/if}
				</div>
			{/each}
		</div>
		<div class="adds">
			{#each KINDS as k (k)}
				<button class="btn" onclick={() => a.steps.push(fresh(k))}>+ {t('step.' + k)}</button>
			{/each}
		</div>
	</div>
	<div class="footer">{t('macro.footer')}</div>
	<datalist id="appList2">
		{#each apps as x (x)}<option value={x}></option>{/each}
	</datalist>
</div>
<TestButton {a} />
