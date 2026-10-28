<!-- Which driver and model the keymap is for, and its layout within that model's limits. -->
<script lang="ts">
	import { ask } from '@tauri-apps/plugin-dialog';
	import { api, message, usbId } from '../api.js';
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { toast } from '../stores/toasts.svelte.js';
	import { slotNames } from '../vocab.js';

	const m = $derived(studio.mapping!);
	const limits = $derived(studio.caps?.layout);
	const device = $derived(studio.device);

	// Drop slots the new layout doesn't have, so the keymap stays valid.
	function prune() {
		const names = new Set(slotNames(m));
		for (const l of m.layers) for (const k of Object.keys(l.slots)) if (!names.has(k)) delete l.slots[k];
		if (!names.has(studio.sel)) studio.select('b0');
	}

	function setNum(field: 'rows' | 'columns' | 'knobs', v: string) {
		const n = Math.round(Number(v));
		const [lo, hi] = limits?.[field] ?? [0, 15];
		if (!Number.isFinite(n) || n < lo || n > hi) return;
		m[field] = n;
		prune();
	}

	async function setType(value: string) {
		const [driver, model] = value.split('/');
		if (driver === m.driver) {
			// Same family: keep the keymap, fit it to the new model's limits.
			m.model = model;
			const caps = studio.caps;
			if (!caps) return;
			const clamp = (v: number, [lo, hi]: [number, number]) => Math.min(hi, Math.max(lo, v));
			m.rows = clamp(m.rows, caps.layout.rows);
			m.columns = clamp(m.columns, caps.layout.columns);
			m.knobs = clamp(m.knobs, caps.layout.knobs);
			m.layers = m.layers.slice(0, caps.max_layers);
			studio.layer = Math.min(studio.layer, m.layers.length - 1);
			if (!caps.led) for (const l of m.layers) delete l.led;
			prune();
			return;
		}
		// Another family speaks a different language: start from its starter keymap.
		if (!(await ask(t('device.switchFamily'), { title: 'OpenKeyForge', kind: 'warning' }))) return;
		try {
			studio.mapping = await api.starterMapping(driver, model);
			studio.layer = 0;
			studio.select('b0');
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}
</script>

<div>
	<div class="sec-h">{t('keypad')}</div>
	<div class="group">
		<div class="row keypad-row">
			<span class="k">{t('keypad.model')}</span>
			<select value={`${m.driver}/${m.model}`} onchange={(e) => setType(e.currentTarget.value)}>
				{#each studio.catalog?.drivers ?? [] as d (d.id)}
					<optgroup label={d.name}>
						{#each d.models as model (model.id)}
							<option value={`${d.id}/${model.id}`}>{model.name}</option>
						{/each}
					</optgroup>
				{/each}
			</select>
		</div>
		{#each [['rows', 'keypad.rows'], ['columns', 'keypad.columns'], ['knobs', 'keypad.knobs']] as const as [field, label] (field)}
			<div class="row keypad-row">
				<span class="k">{t(label)}</span>
				<input
					class="field num"
					type="number"
					min={limits?.[field][0] ?? 0}
					max={limits?.[field][1] ?? 15}
					value={m[field]}
					onchange={(e) => setNum(field, e.currentTarget.value)}
				/>
			</div>
		{/each}
	</div>
	<div class="footer">
		{#if device}{t('keypad.detected', { id: usbId(device), models: device.models.join(' / ') })}{/if}
		{t('keypad.footer')}
	</div>
</div>
