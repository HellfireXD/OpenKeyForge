<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { api, message, onAgentError, onAgentFired, onKeypadEvent } from '../lib/api.js';
	import Device from '../lib/components/Device.svelte';
	import Inspector from '../lib/components/Inspector.svelte';
	import KeypadPanel from '../lib/components/KeypadPanel.svelte';
	import LayerPanel from '../lib/components/LayerPanel.svelte';
	import Toasts from '../lib/components/Toasts.svelte';
	import Toolbar from '../lib/components/Toolbar.svelte';
	import UsagePanel from '../lib/components/UsagePanel.svelte';
	import { LANG, rows, t } from '../lib/i18n.js';
	import { studio } from '../lib/stores/studio.svelte.js';
	import { toast } from '../lib/stores/toasts.svelte.js';
	import { accordId, codeToKey, MEDIA_CODES, MOD_CODES, sortMods } from '../lib/vocab.js';

	let previewDlg = $state<HTMLDialogElement>();
	let helpDlg = $state<HTMLDialogElement>();
	let packets = $state('');
	let loadError = $state<string | null>(null);

	// Save whenever anything in the mapping changes (JSON.stringify reads it all).
	$effect(() => {
		if (!studio.mapping) return;
		JSON.stringify(studio.mapping);
		studio.scheduleSave();
	});

	// The keypad types like a keyboard, so a press that reaches this window is
	// matched against what's on the keypad. Used when the HID watcher can't read
	// the keypad itself (Windows keyboards, macOS without Input Monitoring).
	function keydown(e: KeyboardEvent) {
		if (studio.rawKeys || studio.recording || e.repeat || !studio.uploaded || MOD_CODES[e.code]) return;
		const target = e.target as HTMLElement | null;
		if (target?.closest?.('input, textarea, select, [contenteditable]') || document.querySelector('dialog[open]')) return;
		const media = MEDIA_CODES[e.code];
		if (media) {
			studio.hitMedia(media);
			return;
		}
		const key = codeToKey(e.code);
		if (!key) return;
		const held = [e.ctrlKey && 'ctrl', e.shiftKey && 'shift', e.altKey && 'alt', e.metaKey && 'win'].filter(Boolean) as string[];
		const id = accordId([...sortMods(held), key].join('-'), studio.catalog?.keys ?? []);
		if (!id) return;
		studio.hitKey(id);
		// e.g. Ctrl-S or Ctrl-P shouldn't open the webview's dialogs while testing.
		if (held.length) e.preventDefault();
	}

	async function showPackets() {
		if (!studio.mapping) return;
		try {
			const r = await api.preview($state.snapshot(studio.mapping));
			packets = `# ${r.packets.length} packets × 64 bytes (trailing zeros omitted)\n\n` + r.packets.join('\n');
			previewDlg?.showModal();
		} catch (e) {
			toast(t('preview.fail'), message(e), 'err');
		}
	}

	onMount(() => {
		document.documentElement.lang = LANG;
		const unlisten = [
			onKeypadEvent((e) => studio.rawHit(e)),
			onAgentFired((f) => studio.showHit(f.layer, f.slot)),
			onAgentError((m) => toast(t('agent.actionFail'), m, 'err')),
		];
		let timer: ReturnType<typeof setInterval> | undefined;
		studio
			.load()
			.then(() => {
				studio.poll();
				timer = setInterval(() => studio.poll(), 1500);
			})
			.catch((e) => (loadError = message(e)));
		return () => {
			clearInterval(timer);
			unlisten.forEach((p) => p.then((f) => f()));
		};
	});
</script>

<svelte:window onkeydown={keydown} />

{#if studio.mapping && studio.catalog}
	<Toolbar onhelp={() => helpDlg?.showModal()} onpackets={showPackets} />
	<main>
		<div class="canvas">
			{#if studio.warning}<div class="banner on">{studio.warning}</div>{/if}
			{#if studio.devices && !studio.devices.length && studio.catalog.platform === 'linux'}
				<div class="banner on">{t('dev.linuxHint')}</div>
			{/if}
			<Device />
			<div class="forms">
				<LayerPanel />
				<UsagePanel />
				<KeypadPanel />
			</div>
		</div>
		<aside class="inspector"><Inspector /></aside>
	</main>
	<datalist id="keyList">
		{#each [...studio.catalog.keys, ...studio.catalog.modifiers] as k (k)}<option value={k}></option>{/each}
	</datalist>
{:else if loadError}
	<div class="canvas"><div class="banner on">{loadError}</div></div>
{/if}

<Toasts />

<dialog bind:this={previewDlg}>
	<div class="sheet-b"><h3>{t('menu.packetsTitle')}</h3><pre class="mono">{packets}</pre></div>
	<div class="sheet-f"><button class="btn primary lg" onclick={() => previewDlg?.close()}>{t('done')}</button></div>
</dialog>

<dialog bind:this={helpDlg}>
	<div class="sheet-b">
		<h3>{t('help.title')}</h3>
		<table class="help">
			<tbody>
				{#each rows('help.rows') as [k, v] (k)}
					{@const m = k.match(/^@(dev|agt) (.*)$/)}
					<tr>
						<td>{#if m}<span class="tag"><span class="cat {m[1]}"></span>{m[2]}</span>{:else}{k}{/if}</td>
						<td>{v}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<div class="sheet-f"><button class="btn primary lg" onclick={() => helpDlg?.close()}>{t('done')}</button></div>
</dialog>
