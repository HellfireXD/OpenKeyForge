<script lang="ts">
	import type { Action, ActionType } from '../api.js';
	import { t } from '../i18n.js';
	import { studio } from '../stores/studio.svelte.js';
	import { toast } from '../stores/toasts.svelte.js';
	import { autoLabel, defaultAction, DEV, icon, kindOf, layerName, slotTitle, TYPE_LIST } from '../vocab.js';
	import KeysEditor from './editors/KeysEditor.svelte';
	import MacroEditor from './editors/MacroEditor.svelte';
	import MediaEditor from './editors/MediaEditor.svelte';
	import MouseEditor from './editors/MouseEditor.svelte';
	import RawEditor from './editors/RawEditor.svelte';
	import SoftEditor from './editors/SoftEditor.svelte';

	const a = $derived(studio.action);
	const error = $derived(studio.errors[`${studio.layer}:${studio.sel}`]);
	const picking = $derived(!a || studio.picking || !TYPE_LIST.includes(a.type));

	function pick(ty: ActionType) {
		studio.picking = false;
		if (a?.type !== ty) studio.setSlot({ ...defaultAction(ty), ...(a?.label ? { label: a.label } : {}) });
	}

	function copy() {
		if (!a) return;
		studio.clip = structuredClone($state.snapshot(a)) as Action;
		toast(t('copied'), t('copiedSub'));
	}
</script>

{#snippet tile(ty: ActionType, lg = false)}
	<span class="tile t-{ty}" class:lg>{@html icon(ty)}</span>
{/snippet}

{#snippet option(ty: ActionType)}
	<button class="row opt" aria-pressed={a?.type === ty} onclick={() => pick(ty)}>
		{@render tile(ty)}
		<span class="k"><div>{t('type.' + ty)}</div><div class="d">{t('type.' + ty + '.d')}</div></span>
		<span class="chev">{#if a?.type === ty}{@html icon('check')}{:else}›{/if}</span>
	</button>
{/snippet}

<div class="ins-in">
	<div class="ins-head">
		<div>
			<div class="sub">{studio.current ? layerName(studio.current, studio.layer) : ''}</div>
			<h2>{slotTitle(studio.sel)}</h2>
		</div>
		<div class="tools">
			<button class="btn plain" disabled={!a} onclick={copy}>{t('copy')}</button>
			<button
				class="btn plain"
				disabled={!studio.clip}
				onclick={() => {
					if (studio.clip) studio.setSlot(structuredClone($state.snapshot(studio.clip)) as Action);
					studio.picking = false;
				}}>{t('paste')}</button
			>
			<button
				class="btn plain destructive"
				disabled={!a}
				onclick={() => {
					studio.setSlot(null);
					studio.picking = false;
				}}>{t('clear')}</button
			>
		</div>
	</div>

	{#if picking}
		<div>
			<div class="sec-h">{t('pick.dev')} <small>{t('pick.devSub')}</small></div>
			<div class="group">
				{#each TYPE_LIST.filter((ty) => DEV.has(ty) && (studio.caps?.actions ?? []).includes(ty)) as ty (ty)}{@render option(ty)}{/each}
			</div>
		</div>
		<div>
			<div class="sec-h">{t('pick.agt')} <small>{t('pick.agtSub')}</small></div>
			<div class="group">
				{#each TYPE_LIST.filter((ty) => !DEV.has(ty)) as ty (ty)}{@render option(ty)}{/each}
			</div>
			<div class="footer">{t('pick.agtFooter')}</div>
		</div>
		{#if a && TYPE_LIST.includes(a.type)}
			<div><button class="btn lg" onclick={() => (studio.picking = false)}>{t('cancel')}</button></div>
		{/if}
	{:else if a}
		{@const k = kindOf(a.type)}
		<div>
			<div class="group">
				<div class="row">
					{@render tile(a.type, true)}
					<div class="k">
						<div style="font-weight:600">{t('type.' + a.type)}</div>
						<div class="tag"><span class="cat {k}"></span>{t('kind.' + k)}</div>
					</div>
					<button class="btn" onclick={() => (studio.picking = true)}>{t('change')}</button>
				</div>
				<div class="row">
					<span class="k">{t('keycap')}</span>
					<input
						class="field"
						value={a.label || ''}
						placeholder={autoLabel(a)}
						oninput={(e) => {
							const v = e.currentTarget.value;
							if (v) a.label = v;
							else delete a.label;
						}}
					/>
				</div>
				{#if error}
					<div class="row err">{@html icon('warn')}<span>{error}</span></div>
				{/if}
			</div>
			<div class="footer">
				{#if k === 'dev'}
					{t('note.dev')}
				{:else}
					{t('note.agt')}
					{#if !studio.agent?.enabled}<b>{t('note.agtOff')}</b>{/if}
					{#if studio.agent?.note}<br />{studio.agent.note}{/if}
				{/if}
			</div>
		</div>
		{#key `${studio.layer}:${studio.sel}:${a.type}`}
			{#if a.type === 'keys'}
				<KeysEditor {a} />
			{:else if a.type === 'media'}
				<MediaEditor {a} />
			{:else if a.type === 'mouse'}
				<MouseEditor {a} />
			{:else if a.type === 'raw'}
				<RawEditor {a} />
			{:else if a.type === 'macro'}
				<MacroEditor {a} />
			{:else}
				<SoftEditor {a} />
			{/if}
		{/key}
	{/if}
</div>
