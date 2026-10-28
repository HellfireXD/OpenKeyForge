<script lang="ts">
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { icon, kindOf, label, layerName, ledColor, legendIcon, num, slotTitle } from '../vocab.js';

	let root = $state<HTMLElement>();

	const m = $derived(studio.mapping!);
	const layer = $derived(studio.current!);
	const buttons = $derived(Array.from({ length: num(m.rows) * num(m.columns) }, (_, i) => 'b' + i));
	const knobs = $derived(Array.from({ length: num(m.knobs) }, (_, k) => k));
	const err = (n: string) => !!studio.errors[`${studio.layer}:${n}`];

	// Flash the slot (or its layer tab) that the keypad just fired.
	$effect(() => {
		const h = studio.hit;
		if (!h || !root) return;
		void h.at;
		const el =
			h.layer === studio.layer
				? root.querySelector<HTMLElement>(`[data-slot="${h.slot}"]`)
				: root.querySelector<HTMLElement>(`[data-layer="${h.layer}"]`);
		if (!el) return;
		el.classList.remove('hit');
		void el.offsetWidth;
		el.classList.add('hit');
	});

	function dragstart(e: DragEvent, slot: string) {
		e.dataTransfer?.setData('text/plain', slot);
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
	}

	function drop(e: DragEvent, to: string) {
		e.preventDefault();
		(e.currentTarget as HTMLElement).classList.remove('dragover');
		const from = e.dataTransfer?.getData('text/plain');
		if (!from || from === to || !studio.names.includes(from)) return;
		const s = layer.slots;
		const a = s[from];
		const b = s[to];
		if (b) s[from] = b;
		else delete s[from];
		if (a) s[to] = a;
		else delete s[to];
		studio.select(to);
	}

	const dnd = (slot: string) => ({
		'data-slot': slot,
		draggable: true,
		ondragstart: (e: DragEvent) => dragstart(e, slot),
		ondragover: (e: DragEvent) => {
			e.preventDefault();
			(e.currentTarget as HTMLElement).classList.add('dragover');
		},
		ondragleave: (e: DragEvent) => (e.currentTarget as HTMLElement).classList.remove('dragover'),
		ondrop: (e: DragEvent) => drop(e, slot),
		onclick: () => studio.select(slot),
		'aria-pressed': studio.sel === slot,
		'aria-label': slotTitle(slot),
	});
</script>

<div class="stage" bind:this={root}>
	<div class="device-wrap">
		<div class="device">
			<div class="keys" style:--cols={num(m.columns, 4)}>
				{#each buttons as n, i (n)}
					{@const a = layer.slots[n]}
					<button class="key" class:blank={!a} class:error={err(n)} {...dnd(n)}>
						<span class="meta"><span>{i + 1}</span>{#if a}<span class="cat {kindOf(a.type)}"></span>{/if}</span>
						<span class="legend">
							{#if a}{@html legendIcon(a)}{label(a)}{:else}{t('empty')}{/if}
						</span>
					</button>
				{/each}
			</div>
			{#if knobs.length}
				<div class="knobs">
					{#each knobs as k (k)}
						{@const press = `k${k}.press`}
						{@const pa = layer.slots[press]}
						<div class="knob">
							{#each ['ccw', 'cw'] as act, j (act)}
								{@const n = `k${k}.${act}`}
								{@const a = layer.slots[n]}
								{#if j === 1}
									<button class="crown" class:error={err(press)} title={slotTitle(press)} {...dnd(press)}></button>
								{/if}
								<button class="turn" class:blank={!a} class:error={err(n)} {...dnd(n)}>
									{#if a}<span class="cat {kindOf(a.type)}"></span>{/if}
									<span class="arr">{@html icon(act)}</span>
									<span class="t">{a ? label(a) : t('knob.' + act)}</span>
								</button>
							{/each}
							<div class="press-label">{pa ? label(pa) : t('knob.pressShort')}</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</div>
	<div class="seg lg" role="tablist">
		{#each m.layers as l, i (i)}
			<button
				role="tab"
				data-layer={i}
				aria-selected={i === studio.layer}
				onclick={() => {
					studio.layer = i;
					studio.picking = false;
				}}
			>
				<span class="lamp" style:background={ledColor(l.led)}></span>{layerName(l, i)}
			</button>
		{/each}
	</div>
</div>
