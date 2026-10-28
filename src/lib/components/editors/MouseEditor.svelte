<script lang="ts">
	import type { Action, MouseButtonName, MouseKind } from '../../api.js';
	import { t } from '../../i18n.js';
	import { modSym, num } from '../../vocab.js';

	let { a }: { a: Extract<Action, { type: 'mouse' }> } = $props();

	const ACTIONS: MouseKind[] = ['click', 'wheel', 'move', 'drag'];
	const BUTTONS: MouseButtonName[] = ['left', 'middle', 'right'];

	function setAction(v: MouseKind) {
		a.action = v;
		if (v === 'wheel' || v === 'move') delete a.buttons;
		if (v === 'wheel' && a.delta == null) a.delta = 1;
		if ((v === 'click' || v === 'drag') && !(a.buttons || []).length) a.buttons = ['left'];
	}

	function toggle(b: MouseButtonName) {
		const set = new Set(a.buttons || []);
		if (set.has(b)) set.delete(b);
		else set.add(b);
		a.buttons = [...set];
	}

	const clamp = (v: string) => Math.max(-127, Math.min(127, num(v)));
</script>

<div>
	<div class="sec-h">{t('mouse.title')}</div>
	<div class="group">
		<div class="row">
			<span class="k">{t('mouse.action')}</span>
			<div class="seg">
				{#each ACTIONS as k (k)}
					<button aria-pressed={a.action === k} onclick={() => setAction(k)}>{t('mouse.' + k)}</button>
				{/each}
			</div>
		</div>
		{#if a.action === 'click' || a.action === 'drag'}
			<div class="row">
				<span class="k">{t('mouse.buttons')}</span>
				<div class="seg">
					{#each BUTTONS as b (b)}
						<button aria-pressed={(a.buttons || []).includes(b)} onclick={() => toggle(b)}>{t('mouse.' + b)}</button>
					{/each}
				</div>
			</div>
		{/if}
		{#if a.action === 'wheel'}
			<div class="stack">
				<div class="k"><span>{t('mouse.amount')}</span><small>{t('mouse.amountSub')}</small></div>
				<div class="range">
					<input type="range" min="-20" max="20" value={num(a.delta, 1)} oninput={(e) => (a.delta = clamp(e.currentTarget.value))} />
					<input class="field num" type="number" min="-127" max="127" value={num(a.delta, 1)} oninput={(e) => (a.delta = clamp(e.currentTarget.value))} />
				</div>
			</div>
		{/if}
		{#if a.action === 'move' || a.action === 'drag'}
			<div class="row">
				<span class="k">X</span>
				<input class="field num" type="number" min="-127" max="127" value={num(a.dx)} oninput={(e) => (a.dx = clamp(e.currentTarget.value))} />
				<span class="unit">px</span>
			</div>
			<div class="row">
				<span class="k">Y</span>
				<input class="field num" type="number" min="-127" max="127" value={num(a.dy)} oninput={(e) => (a.dy = clamp(e.currentTarget.value))} />
				<span class="unit">px</span>
			</div>
		{/if}
		<div class="row">
			<span class="k">{t('mouse.mod')}</span>
			<div class="seg">
				{#each [['', t('mouse.none')], ['ctrl', modSym('ctrl')], ['shift', modSym('shift')], ['alt', modSym('alt')]] as [k, v] (k)}
					<button aria-pressed={(a.modifier || '') === k} onclick={() => (a.modifier = k)}>{v}</button>
				{/each}
			</div>
		</div>
	</div>
	<div class="footer">{t('mouse.footer')}</div>
</div>
