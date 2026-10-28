<script lang="ts">
	import { ask } from '@tauri-apps/plugin-dialog';
	import { openUrl } from '@tauri-apps/plugin-opener';
	import { api, message, usbId, type Mapping } from '../api.js';
	import { LANG, setLang, t } from '../i18n.js';
	import { stopRecording } from '../recorder.js';
	import { studio } from '../stores/studio.svelte.js';
	import { toast } from '../stores/toasts.svelte.js';
	import { icon, riskyActions } from '../vocab.js';

	let {
		onhelp,
		onpackets,
	}: {
		onhelp: () => void;
		onpackets: () => void;
	} = $props();

	let menuOpen = $state(false);
	let fresh = $state(false);
	let fadeTimer: ReturnType<typeof setTimeout> | undefined;

	const device = $derived(studio.device);
	const agentOn = $derived(!!studio.agent?.enabled);

	$effect(() => {
		if (!studio.hit) return;
		void studio.hit.at;
		fresh = true;
		clearTimeout(fadeTimer);
		fadeTimer = setTimeout(() => (fresh = false), 3000);
	});

	const hitText = $derived(
		studio.hit
			? t('hit.pressed', { where: studio.hit.where })
			: studio.devices?.length && studio.uploaded
				? t('hit.hint')
				: '',
	);

	async function upload() {
		if (!studio.mapping) return;
		stopRecording();
		await studio.save();
		studio.uploading = true;
		try {
			const snapshot = $state.snapshot(studio.mapping) as Mapping;
			const r = await api.uploadConfig(studio.profileId, studio.configId);
			studio.uploaded = structuredClone(snapshot);
			toast(t('up.ok'), t('up.okSub', { hw: r.hardware, sw: r.software }), 'ok');
			if (r.software && !studio.agent?.enabled) toast(t('up.agentOff'), t('up.agentOffSub'));
		} catch (e) {
			toast(t('up.fail'), message(e), 'err');
		}
		studio.uploading = false;
	}

	async function toggleAgent() {
		try {
			studio.agent = await api.setAgent(!agentOn);
			const on = studio.agent.enabled;
			toast(on ? t('agent.on') : t('agent.off'), on ? t('agent.onSub') : t('agent.offSub'), 'ok');
			if (studio.agent.problem) toast(t('agent.fail'), studio.agent.problem, 'err');
		} catch (e) {
			toast(t('agent.fail'), message(e), 'err');
		}
	}

	async function importMapping() {
		menuOpen = false;
		try {
			const m = await api.importMapping();
			if (!m) return;
			// A shared mapping can hide commands that run as you once uploaded. Show them before accepting.
			const risky = riskyActions(m);
			if (
				risky.length &&
				!(await ask(
					t('imp.risky') +
						'\n\n' +
						risky.slice(0, 15).join('\n') +
						(risky.length > 15 ? `\n… (+${risky.length - 15})` : ''),
					{ title: 'OpenKeyForge', kind: 'warning' },
				))
			)
				return;
			for (const l of m.layers) l.slots ??= {};
			studio.mapping = m;
			studio.layer = 0;
			studio.sel = 'b0';
			studio.picking = false;
			toast(t('imp.ok'), t('imp.okSub'), 'ok');
		} catch (e) {
			toast(t('imp.fail'), message(e), 'err');
		}
	}

	async function exportMapping() {
		menuOpen = false;
		if (!studio.mapping) return;
		try {
			if (await api.exportMapping($state.snapshot(studio.mapping))) toast(t('exp.ok'), '', 'ok');
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async function readDevice() {
		menuOpen = false;
		if (!studio.mapping) return;
		if (!studio.device) {
			toast(t('read.fail'), t('dev.off'), 'err');
			return;
		}
		try {
			const m = await api.readFromDevice(studio.mapping.driver, studio.device.path);
			studio.mapping = m;
			studio.uploaded = structuredClone(m);
			studio.layer = 0;
			studio.select('b0');
			toast(t('read.ok'), '', 'ok');
		} catch (e) {
			toast(t('read.fail'), message(e), 'err');
		}
	}

	async function toggleTray() {
		menuOpen = false;
		const on = !studio.settings.tray;
		try {
			await api.setTray(on);
			studio.settings.tray = on;
			toast(on ? t('tray.on') : t('tray.off'), on ? t('tray.onSub') : '', 'ok');
		} catch (e) {
			toast(t('tray.fail'), message(e), 'err');
		}
	}

	async function switchLang() {
		await studio.save();
		setLang(LANG === 'ko' ? 'en' : 'ko');
		location.reload();
	}
</script>

<svelte:document onclick={() => (menuOpen = false)} />

<div class="toolbar">
	<div class="app"><span class="app-icon"><i></i><i></i><i></i><i></i></span>OpenKeyForge</div>
	<span class="hit-status" class:fresh aria-live="polite" title={studio.hit?.where ?? ''}>{hitText}</span>
	<span class="status" title={device ? `${device.name} (${usbId(device)})` : ''}>
		{#if studio.devices === null}
			<span class="dot"></span>{t('dev.checking')}
		{:else if device}
			<span class="dot on"></span>{t('dev.on')}
		{:else}
			<span class="dot off"></span>{t('dev.off')}
		{/if}
	</span>
	<span class="divider"></span>
	<button
		class="switch-row"
		role="switch"
		aria-checked={agentOn}
		title={studio.agent?.note ?? t('agent.tip')}
		disabled={studio.agent ? !studio.agent.available : false}
		onclick={toggleAgent}
	>
		<span class="switch"></span><span>{t('agent.name')}</span>
	</button>
	<div class="menu">
		<button
			class="icon-btn"
			aria-label="Menu"
			onclick={(e) => {
				e.stopPropagation();
				menuOpen = !menuOpen;
			}}
		>
			{@html icon('dots')}
		</button>
		<div class="menu-pop" class:open={menuOpen}>
			<button onclick={() => ((menuOpen = false), onhelp())}>{t('menu.help')}</button>
			{#if studio.caps?.packet_preview}
				<button onclick={() => ((menuOpen = false), onpackets())}>{t('menu.packets')}</button>
			{/if}
			{#if studio.caps?.read_keymap}
				<button onclick={readDevice}>{t('menu.read')}</button>
			{/if}
			<hr />
			<button role="menuitemcheckbox" aria-checked={studio.settings.tray} onclick={toggleTray}
				>{studio.settings.tray ? '✓ ' : ''}{t('menu.tray')}</button
			>
			<hr />
			<button onclick={exportMapping}>{t('menu.export')}</button>
			<button onclick={importMapping}>{t('menu.import')}</button>
			<hr />
			<button lang={LANG === 'ko' ? 'en' : 'ko'} onclick={switchLang}>{LANG === 'ko' ? 'English' : '한국어'}</button>
			<hr />
			<button onclick={() => openUrl('https://github.com/HellfireXD/OpenKeyForge')}>{t('menu.github')}</button>
		</div>
	</div>
	<button
		class="btn lg"
		class:primary={studio.dirty && !studio.errorCount && !studio.uploading}
		disabled={studio.errorCount > 0 || studio.uploading}
		title={studio.errorCount ? t('up.tipErr') : studio.dirty ? t('up.tipDirty') : t('up.tipSynced')}
		onclick={upload}
	>
		{#if studio.uploading}
			{t('up.busy')}
		{:else if studio.errorCount}
			{t('up.errors', { n: studio.errorCount })}
		{:else if studio.dirty}
			{t('up.dirty')}
		{:else}
			{@html icon('check')} {t('up.synced')}
		{/if}
	</button>
</div>
