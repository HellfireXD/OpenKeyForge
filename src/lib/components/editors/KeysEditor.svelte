<script lang="ts">
	import type { Action } from '../../api.js';
	import { t } from '../../i18n.js';
	import { recorder } from '../../recorder.js';
	import { studio } from '../../stores/studio.svelte.js';
	import { num, prettyAccord } from '../../vocab.js';

	let { a }: { a: Extract<Action, { type: 'keys' }> } = $props();

	let input = $state<HTMLInputElement>();
	const max = $derived(studio.caps?.max_steps ?? 18);
	const maxDelay = $derived(studio.caps?.max_delay ?? 0);

	function add(steps: string[]) {
		for (const s of steps) if (a.steps.length < max) a.steps.push(s.toLowerCase());
	}

	function keydown(e: KeyboardEvent & { currentTarget: HTMLInputElement }) {
		const el = e.currentTarget;
		if (e.key === 'Enter' && el.value.trim()) {
			e.preventDefault();
			add(el.value.split(',').map((s) => s.trim()).filter(Boolean));
			el.value = '';
		} else if (e.key === 'Backspace' && !el.value && a.steps.length) {
			a.steps.pop();
		}
	}

	function setDelay(v: string) {
		a.delay = Math.max(0, Math.min(maxDelay, num(v)));
	}
</script>

<div>
	<div class="sec-h">{t('keys.title')} <small>{a.steps.length} / {max}</small></div>
	<div class="group">
		<div class="stack">
			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="chips" onclick={() => input?.focus()}>
				{#each a.steps as s, i (i)}
					{#if i}<span class="then">›</span>{/if}
					<span class="kbd" title={s}>
						<span class="n">{i + 1}</span>{prettyAccord(s)}
						<button aria-label={t('macro.del')} onclick={() => a.steps.splice(i, 1)}>✕</button>
					</span>
				{/each}
				<input bind:this={input} list="keyList" placeholder={a.steps.length ? '' : t('keys.ph')} onkeydown={keydown} />
			</div>
			<div class="hrow" style="margin-top:9px">
				<button class="btn rec" use:recorder={(acc) => add([acc])}><span class="rd"></span><span>{t('keys.rec')}</span></button>
				<span class="unit">{t('keys.recHint')}</span>
			</div>
		</div>
	</div>
	<div class="footer">{@html t('keys.footer')}</div>
</div>

{#if maxDelay > 0}
<div>
	<div class="sec-h">{t('keys.delay')}</div>
	<div class="group">
		<div class="stack">
			<div class="range">
				<input type="range" min="0" max={maxDelay} step="10" value={num(a.delay)} oninput={(e) => setDelay(e.currentTarget.value)} />
				<input class="field num" type="number" min="0" max={maxDelay} value={num(a.delay)} oninput={(e) => setDelay(e.currentTarget.value)} />
				<span class="unit">ms</span>
			</div>
		</div>
	</div>
	<div class="footer">{t('keys.delayFooter')}</div>
</div>
{/if}
