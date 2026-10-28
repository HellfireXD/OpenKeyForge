<script lang="ts">
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { LED_HEX } from '../vocab.js';

	const l = $derived(studio.current!);
	const [mode, color] = $derived(String(l.led || 'off').split(' '));
	const led = $derived(studio.caps?.led ?? null);
	const ledKnown = $derived(!!led);

	function setMode(m: string) {
		let c = color || 'white';
		if (m !== 'backlight' && c === 'white') c = 'blue';
		l.led = m === 'off' ? 'off' : `${m} ${c}`;
	}
</script>

<div>
	<div class="sec-h">{t('layer', { n: studio.layer + 1 })}</div>
	<div class="group">
		<div class="row">
			<span class="k">{t('name')}</span>
			<input
				class="field"
				value={l.name || ''}
				placeholder={t('layer', { n: studio.layer + 1 })}
				oninput={(e) => {
					const v = e.currentTarget.value;
					if (v) l.name = v;
					else delete l.name;
				}}
			/>
		</div>
		{#if ledKnown}
			<div class="row">
				<span class="k">{t('led')}</span>
				<div class="seg">
					{#each led?.modes ?? [] as m (m)}
						<button aria-pressed={m === mode} onclick={() => setMode(m)}>{t('led.' + m)}</button>
					{/each}
				</div>
			</div>
			<div class="row">
				<span class="k">{t('color')}</span>
				<div class="swatches">
					{#each led?.colors ?? [] as c (c)}
						<button
							class="swatch"
							title={c}
							aria-label={c}
							style:background={LED_HEX[c] || '#888'}
							aria-pressed={mode !== 'off' && (color || 'white') === c}
							disabled={mode === 'off' || (c === 'white' && mode !== 'backlight')}
							onclick={() => (l.led = `${mode} ${c}`)}
						></button>
					{/each}
				</div>
			</div>
		{/if}
	</div>
	<div class="footer">{ledKnown ? t('led.footer') : t('led.unknown')}</div>
</div>
