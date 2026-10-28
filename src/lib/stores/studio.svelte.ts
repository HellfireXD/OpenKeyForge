// The editor's state: the mapping being edited, what's on the keypad, the
// selection, and what the backend reports about devices and the agent.
import { api, message, type Action, type AgentStatus, type Catalog, type Device, type KeypadEvent, type Mapping } from '../api.js';
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
	mapping = $state<Mapping | null>(null);
	uploaded = $state<Mapping | null>(null);
	errors = $state<Record<string, string>>({});
	catalog = $state<Catalog | null>(null);
	warning = $state<string | null>(null);

	layer = $state(0);
	sel = $state('b0');
	clip = $state<Action | null>(null);
	picking = $state(false);
	uploading = $state(false);

	devices = $state<Device[] | null>(null);
	/** The first connected device the current driver can program. */
	device = $derived(this.devices?.find((d) => d.driver === this.mapping?.driver));
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
		for (const l of st.mapping.layers) l.slots ??= {};
		this.catalog = st.catalog;
		this.mapping = st.mapping;
		this.uploaded = st.uploaded;
		this.errors = st.errors;
		this.warning = st.warning;
		this.#lastSaved = JSON.stringify(st.mapping);
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
		try {
			const r = await api.saveMapping(JSON.parse(json));
			this.#lastSaved = json;
			this.errors = r.errors;
		} catch (e) {
			toast(t('save.fail'), message(e), 'err');
		}
	}

	async poll(): Promise<void> {
		try {
			const r = await api.status();
			const seen = this.agent?.last?.at ?? null;
			this.devices = r.devices;
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
