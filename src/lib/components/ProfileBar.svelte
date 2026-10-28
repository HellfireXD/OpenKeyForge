<!-- Profiles ("Halo", "Adobe") and the device configs in the selected one. -->
<script lang="ts">
	import { ask } from '@tauri-apps/plugin-dialog';
	import { api, message, usbId, type Attached, type DeviceConfig, type DeviceRef } from '../api.js';
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { toast } from '../stores/toasts.svelte.js';

	let mode = $state<'idle' | 'new' | 'rename'>('idle');
	let name = $state('');
	let copy = $state(true);
	let adding = $state(false);
	let applying = $state(false);

	const profile = $derived(studio.profile);
	const config = $derived(studio.config);
	const devices = $derived(studio.devices ?? []);

	const sameRef = (a: DeviceRef | null, b: DeviceRef | null) =>
		!!a && !!b && a.vendor_id === b.vendor_id && a.product_id === b.product_id && (a.serial ?? null) === (b.serial ?? null) && a.index === b.index;
	const hex = (n: number) => n.toString(16).padStart(4, '0');

	function bindLabel(c: DeviceConfig): string {
		if (!c.bind) return t('config.anyLabel', { model: c.keymap.model });
		const id = `${hex(c.bind.vendor_id)}:${hex(c.bind.product_id)}`;
		return c.bind.serial ? `${id} · ${c.bind.serial}` : t('config.boundLabel', { id, n: c.bind.index + 1 });
	}

	const connected = (c: DeviceConfig) => {
		const target = studio.targets[studio.profileId]?.[c.id];
		return !!target && 'Ok' in target;
	};

	const configName = (c: DeviceConfig, i: number) => c.name || t('config.device', { n: i + 1 });

	function startEdit(m: 'new' | 'rename') {
		mode = m;
		name = m === 'rename' ? (profile?.name ?? '') : '';
		copy = true;
	}

	async function submit() {
		try {
			if (mode === 'new') {
				const p = await api.createProfile(name, copy ? studio.profileId : null);
				studio.profiles.profiles.push(p);
				await studio.open(p.id);
			} else if (mode === 'rename' && profile) {
				await api.renameProfile(profile.id, name);
				profile.name = name.trim();
			}
			mode = 'idle';
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async function remove() {
		if (!profile) return;
		if (!(await ask(t('profile.deleteAsk', { name: profile.name }), { title: 'OpenKeyForge', kind: 'warning' }))) return;
		try {
			await api.deleteProfile(profile.id);
			studio.profiles.profiles = studio.profiles.profiles.filter((p) => p.id !== profile.id);
			if (studio.profiles.active === profile.id) studio.profiles.active = null;
			await studio.open(studio.profiles.profiles[0].id);
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async function apply() {
		if (!profile) return;
		await studio.save();
		applying = true;
		try {
			const results = await api.applyProfile(profile.id);
			studio.profiles.active = profile.id;
			const ok = results.filter((r) => !r.error).length;
			const failed = results.filter((r) => r.error);
			toast(t('profile.applied', { name: profile.name }), t('profile.appliedSub', { ok, skipped: failed.length }), ok ? 'ok' : 'err');
			for (const r of failed) {
				const i = profile.configs.findIndex((c) => c.id === r.config);
				if (i >= 0) toast(configName(profile.configs[i], i), r.error ?? '', 'err');
			}
		} catch (e) {
			toast(t('profile.applyFail', { name: profile.name }), message(e), 'err');
		}
		applying = false;
	}

	async function addConfig(device: Attached | null) {
		adding = false;
		if (!profile) return;
		const driver = device?.driver ?? studio.mapping?.driver ?? 'ch57x';
		const model = device?.models[0] ?? studio.mapping?.model ?? 'ch57x-1';
		try {
			const c = await api.addConfig(profile.id, device?.bind ?? null, driver, model);
			profile.configs.push(c);
			await studio.open(profile.id, c.id);
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async function updateConfig(next: { name?: string | null; bind?: DeviceRef | null }) {
		if (!profile || !config) return;
		const n = next.name !== undefined ? next.name : (config.name ?? null);
		const b = next.bind !== undefined ? next.bind : config.bind;
		try {
			await api.updateConfig(profile.id, config.id, n, b);
			config.name = n ?? undefined;
			config.bind = b;
			void studio.poll();
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async function removeConfig() {
		if (!profile || !config) return;
		try {
			await api.removeConfig(profile.id, config.id);
			profile.configs = profile.configs.filter((c) => c.id !== config.id);
			await studio.open(profile.id);
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	function onBind(value: string) {
		if (value === 'any') return updateConfig({ bind: null });
		const d = devices.find((x) => x.key === value);
		if (d) return updateConfig({ bind: d.bind });
	}
</script>

<svelte:document onclick={() => (adding = false)} />

{#if profile}
	<div class="profile-bar">
		<div class="group">
			<div class="row">
				<span class="k">{t('profile')}</span>
				{#if mode !== 'idle'}
					<input
						class="field profile-name"
						placeholder={t('profile.namePh')}
						bind:value={name}
						onkeydown={(e) => {
							if (e.key === 'Enter') submit();
							if (e.key === 'Escape') mode = 'idle';
						}}
					/>
					{#if mode === 'new'}
						<label class="unit"><input type="checkbox" bind:checked={copy} /> {t('profile.copy')}</label>
					{/if}
					<button class="btn" disabled={!name.trim()} onclick={submit}>{mode === 'new' ? t('profile.create') : t('profile.save')}</button>
					<button class="btn plain" onclick={() => (mode = 'idle')}>{t('cancel')}</button>
				{:else}
					<select class="profile-select" value={studio.profileId} onchange={(e) => studio.open(e.currentTarget.value)}>
						{#each studio.profiles.profiles as p (p.id)}
							<option value={p.id}>{p.name}{studio.profiles.active === p.id ? ` (${t('profile.active')})` : ''}</option>
						{/each}
					</select>
					<button class="btn plain" onclick={() => startEdit('new')}>{t('profile.new')}</button>
					<button class="btn plain" onclick={() => startEdit('rename')}>{t('profile.rename')}</button>
					<button class="btn plain destructive" disabled={studio.profiles.profiles.length <= 1} onclick={remove}>{t('profile.delete')}</button>
					<span class="sp"></span>
					<button class="btn primary" title={t('profile.applyTip')} disabled={applying} onclick={apply}>{t('profile.apply')}</button>
				{/if}
			</div>
			<div class="row chips-row">
				{#each profile.configs as c, i (c.id)}
					<button class="chip" aria-pressed={c.id === studio.configId} onclick={() => studio.open(profile.id, c.id)}>
						<span class="dot" class:on={connected(c)} class:off={!connected(c)}></span>
						<span>{configName(c, i)}</span>
						<small>{bindLabel(c)}</small>
					</button>
				{/each}
				<div class="menu">
					<button
						class="btn plain"
						onclick={(e) => {
							e.stopPropagation();
							adding = !adding;
						}}>+ {t('config.add')}</button
					>
					<div class="menu-pop" class:open={adding}>
						{#each devices as d (d.key)}
							<button onclick={() => addConfig(d)}>{d.name} · {usbId(d)}{d.bind.serial ? ` · ${d.bind.serial}` : ` · #${d.bind.index + 1}`}</button>
						{/each}
						{#if devices.length}<hr />{/if}
						<button onclick={() => addConfig(null)}>{t('config.any')}</button>
					</div>
				</div>
			</div>
			{#if config}
				<div class="row">
					<span class="k">{t('config.name')}</span>
					<input
						class="field"
						value={config.name ?? ''}
						placeholder={configName(config, profile.configs.indexOf(config))}
						onchange={(e) => updateConfig({ name: e.currentTarget.value })}
					/>
				</div>
				<div class="row keypad-row">
					<span class="k">{t('config.bind')}</span>
					<select
						value={config.bind ? (devices.find((d) => sameRef(d.bind, config.bind))?.key ?? 'current') : 'any'}
						onchange={(e) => onBind(e.currentTarget.value)}
					>
						<option value="any">{t('config.any')}</option>
						{#if config.bind && !devices.some((d) => sameRef(d.bind, config.bind))}
							<option value="current">{bindLabel(config)} ({t('config.missing')})</option>
						{/if}
						{#each devices as d (d.key)}
							<option value={d.key}>{d.name} · {usbId(d)}{d.bind.serial ? ` · ${d.bind.serial}` : ` · #${d.bind.index + 1}`}</option>
						{/each}
					</select>
					<span class="unit">{connected(config) ? t('config.connected') : t('config.missing')}</span>
				</div>
				{#if profile.configs.length > 1}
					<div class="row"><button class="btn plain destructive" onclick={removeConfig}>{t('config.remove')}</button></div>
				{/if}
			{/if}
		</div>
		<div class="footer">{t('config.footer')}</div>
	</div>
{/if}
