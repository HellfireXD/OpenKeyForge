// The editor's state: the mapping being edited, what's on the keypad, the
// selection, and what the backend reports about devices and the agent.
import {
	api,
	message,
	type Action,
	type AgentStatus,
	type Attached,
	type Catalog,
	type KeypadEvent,
	type Mapping,
	type Profiles,
	type Settings,
	type Status,
} from '../api.js';
import { t } from '../i18n.js';
import {
	accordId,
	deviceView,
	label,
	layerName,
	MEDIA_USAGE,
	mediaCodeOf,
	num,
	setPlatform,
	slotNames,
	slotTitle,
} from '../vocab.js';
import { toast } from './toasts.svelte.js';

export interface Hit {
	layer: number;
	slot: string;
	n: number;
	at: number;
	where: string;
}

class Studio {
	/** Every profile; the editor works on one device config of one profile. */
	profiles = $state<Profiles>({ active: null, profiles: [] });
	/** What each config last wrote to its device, by config id. */
	applied = $state<Record<string, Mapping>>({});
	settings = $state<Settings>({ agent: false, tray: false });
	profileId = $state('');
	configId = $state('');
	profile = $derived(this.profiles.profiles.find((p) => p.id === this.profileId));
	config = $derived(this.profile?.configs.find((c) => c.id === this.configId));

	/** The keymap being edited: the selected config's. */
	get mapping(): Mapping | null {
		return this.config?.keymap ?? null;
	}
	set mapping(m: Mapping | null) {
		if (this.config && m) this.config.keymap = m;
	}

	/** What the selected config's device holds, if it was written from this config. */
	get uploaded(): Mapping | null {
		return this.config ? (this.applied[this.config.id] ?? null) : null;
	}
	set uploaded(m: Mapping | null) {
		if (!this.config) return;
		if (m) this.applied[this.config.id] = m;
		else delete this.applied[this.config.id];
	}

	errors = $state<Record<string, string>>({});
	catalog = $state<Catalog | null>(null);
	warning = $state<string | null>(null);

	layer = $state(0);
	sel = $state('b0');
	clip = $state<Action | null>(null);
	picking = $state(false);
	uploading = $state(false);

	devices = $state<Attached[] | null>(null);
	/** profile id → config id → device key, or why the config has no device. */
	targets = $state<Status['targets']>({});
	/** Where the selected config would be written right now. */
	target = $derived(this.targets[this.profileId]?.[this.configId]);
	/** The attached device the selected config would be written to. */
	device = $derived(
		this.target && 'Ok' in this.target ? this.devices?.find((d) => 'Ok' in this.target! && d.key === this.target.Ok) : undefined,
	);
	agent = $state<AgentStatus | null>(null);
	watching = $state(false);
	hit = $state<Hit | null>(null);
	/** Set once raw key events arrive, so window key presses aren't counted twice. */
	rawKeys = false;
	/** Ignore the rest of a multi-step sequence so later steps don't read as other keys. */
	hitQuiet = 0;
	recording = false;

	/** The driver and model the mapping is for, and what that model can do. */
	driverInfo = $derived(this.catalog?.drivers.find((d) => d.id === this.mapping?.driver));
	modelInfo = $derived(this.driverInfo?.models.find((m) => m.id === this.mapping?.model));
	caps = $derived(this.modelInfo?.capabilities);
	current = $derived(this.mapping?.layers[this.layer]);
	action = $derived(this.current?.slots[this.sel]);
	names = $derived(this.mapping ? slotNames(this.mapping) : []);
	dirty = $derived(deviceView(this.mapping) !== deviceView(this.uploaded));
	errorCount = $derived(Object.keys(this.errors).length);

	#saveTimer: ReturnType<typeof setTimeout> | undefined;
	#lastSaved = '';

	async load(): Promise<void> {
		const st = await api.getState();
		// Keycap text depends on the platform; set it before anything renders.
		setPlatform(st.catalog.platform);
		for (const p of st.profiles.profiles) for (const c of p.configs) for (const l of c.keymap.layers) l.slots ??= {};
		this.catalog = st.catalog;
		this.settings = st.settings;
		this.applied = st.applied;
		this.profiles = st.profiles;
		this.warning = st.warning;
		const first = st.profiles.profiles.find((p) => p.id === st.profiles.active) ?? st.profiles.profiles[0];
		await this.open(first.id, first.configs[0].id);
	}

	/** Switches the editor to another profile and device config. */
	async open(profileId: string, configId?: string): Promise<void> {
		await this.save();
		const profile = this.profiles.profiles.find((p) => p.id === profileId);
		if (!profile) return;
		this.profileId = profile.id;
		this.configId = profile.configs.find((c) => c.id === configId)?.id ?? profile.configs[0].id;
		this.layer = 0;
		this.select('b0');
		this.#lastSaved = JSON.stringify(this.mapping);
		await this.refreshErrors();
	}

	async refreshErrors(): Promise<void> {
		if (!this.mapping) return;
		try {
			this.errors = await api.checkKeymap($state.snapshot(this.mapping) as Mapping);
		} catch (e) {
			this.errors = {};
			toast(t('save.fail'), message(e), 'err');
		}
	}

	/** Takes the backend's copy after a profile change made elsewhere (tray, other edits). */
	takeProfiles(p: Profiles): void {
		// Keep the keymap being edited: the backend copy may predate the latest keystroke.
		const editing = this.config ? $state.snapshot(this.config.keymap) : null;
		this.profiles = p;
		if (editing && this.config) this.config.keymap = editing as Mapping;
		if (!this.profile) {
			const first = p.profiles[0];
			if (first) void this.open(first.id);
		} else if (!this.config) {
			this.configId = this.profile.configs[0].id;
		}
	}

	select(name: string): void {
		this.sel = name;
		this.picking = false;
	}

	setSlot(a: Action | null): void {
		const l = this.current;
		if (!l) return;
		if (a) l.slots[this.sel] = a;
		else delete l.slots[this.sel];
	}

	/** Debounced save; called by an effect whenever the mapping changes. */
	scheduleSave(): void {
		if (!this.mapping) return;
		const json = JSON.stringify(this.mapping);
		if (json === this.#lastSaved) return;
		clearTimeout(this.#saveTimer);
		this.#saveTimer = setTimeout(() => this.save(json), 250);
	}

	async save(json = JSON.stringify(this.mapping)): Promise<void> {
		clearTimeout(this.#saveTimer);
		if (!this.mapping || json === this.#lastSaved) return;
		const [profile, config] = [this.profileId, this.configId];
		try {
			this.errors = await api.saveConfig(profile, config, JSON.parse(json));
			this.#lastSaved = json;
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async poll(): Promise<void> {
		try {
			const r = await api.status();
			const seen = this.agent?.last?.at ?? null;
			this.devices = r.devices;
			this.targets = r.targets;
			this.agent = r.agent;
			this.watching = r.watching;
			// Fallback for missed events: show an agent action newer than the last poll.
			const last = r.agent.last;
			if (seen !== null && last && last.at > seen) this.showHit(last.layer, last.slot);
		} catch {
			this.devices = [];
		}
	}

	/* ---------- live key test ---------- */

	showHit(layer: number, slot: string): void {
		const l = this.uploaded?.layers?.[layer];
		const a = l?.slots?.[slot];
		if (!l || !a) return;
		const now = performance.now();
		const again = this.hit?.layer === layer && this.hit?.slot === slot && now - this.hit.at < 1500;
		const n = again && this.hit ? this.hit.n + 1 : 1;
		this.hit = {
			layer,
			slot,
			n,
			at: now,
			where: `${layerName(l, layer)} · ${slotTitle(slot)} · ${label(a)}${n > 1 ? ` ×${n}` : ''}`,
		};
	}

	findHit(match: (a: Action) => boolean): Action | null {
		const hits: [number, string, Action][] = [];
		this.uploaded?.layers.forEach((l, li) =>
			Object.entries(l.slots || {}).forEach(([n, a]) => {
				if (match(a)) hits.push([li, n, a]);
			}),
		);
		if (!hits.length) return null;
		// The keypad's active layer is unknown: prefer the layer on screen, then the previous press's.
		const rank = (li: number) => (li === this.layer ? 0 : li === this.hit?.layer ? 1 : 2);
		hits.sort((x, y) => rank(x[0]) - rank(y[0]));
		this.showHit(hits[0][0], hits[0][1]);
		return hits[0][2];
	}

	hitMedia(code: string | number): Action | null {
		const want = mediaCodeOf(code, MEDIA_USAGE);
		return this.findHit((a) => a.type === 'media' && mediaCodeOf(a.code, MEDIA_USAGE) === want);
	}

	hitKey(id: string): void {
		if (performance.now() < this.hitQuiet) return;
		const keys = this.catalog?.keys ?? [];
		const a = this.findHit((a) => a.type === 'keys' && accordId(a.steps?.[0], keys) === id);
		if (a && a.type === 'keys') this.hitQuiet = performance.now() + 150 + (a.steps.length - 1) * (num(a.delay) + 40);
	}

	/** What the keypad itself sent (src-tauri/src/hid.rs). */
	rawHit(ev: KeypadEvent): void {
		const mouse = (a: Action, ...acts: string[]) => a.type === 'mouse' && acts.includes(a.action);
		if (ev.kind === 'media') this.hitMedia(ev.code);
		else if (ev.kind === 'wheel') this.findHit((a) => mouse(a, 'wheel') && a.type === 'mouse' && Math.sign(num(a.delta)) === ev.delta);
		else if (ev.kind === 'button')
			this.findHit((a) => a.type === 'mouse' && mouse(a, 'click', 'drag') && (a.buttons || []).includes(ev.button));
		else if (ev.kind === 'move')
			this.findHit((a) => a.type === 'mouse' && mouse(a, 'move', 'drag') && num(ev.axis === 'x' ? a.dx : a.dy) === ev.delta);
		else if (ev.kind === 'key') {
			this.rawKeys = true;
			this.hitKey(`${(ev.mods | (ev.mods >> 4)) & 0x0f}:${ev.code}`);
		}
	}
}

export const studio = new Studio();
