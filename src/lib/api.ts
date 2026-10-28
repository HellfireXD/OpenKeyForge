// Types and commands shared with src-tauri/src/lib.rs. The mapping types
// mirror crates/ch57x/src/mapping.rs (serde field names).
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type MouseKind = 'click' | 'move' | 'drag' | 'wheel';
export type MouseButtonName = 'left' | 'right' | 'middle';

export type MacroStep =
	| { do: 'keys'; value: string; repeat?: number }
	| { do: 'text'; value: string; repeat?: number }
	| { do: 'wait'; ms: number }
	| { do: 'open'; value: string; repeat?: number }
	| { do: 'shell'; value: string; repeat?: number };

export type ActionKind =
	| { type: 'keys'; steps: string[]; delay?: number }
	| { type: 'media'; code: string | number }
	| {
			type: 'mouse';
			action: MouseKind;
			buttons?: MouseButtonName[];
			modifier?: string;
			dx?: number;
			dy?: number;
			delta?: number;
	  }
	| { type: 'raw'; expr: string }
	| { type: 'text'; text: string }
	| { type: 'open'; target: string }
	| { type: 'shell'; command: string }
	| { type: 'macro'; steps: MacroStep[] };

export type ActionType = ActionKind['type'];
export type Action = ActionKind & { label?: string };

export interface Layer {
	name?: string;
	led?: string;
	slots: Record<string, Action>;
}

export interface Mapping {
	/** Driver id (crates/drivers/*), e.g. "ch57x". */
	driver: string;
	/** One of that driver's model ids. */
	model: string;
	rows: number;
	columns: number;
	knobs: number;
	layers: Layer[];
}

/** What a device model supports (crates/core/src/driver.rs). */
export interface Capabilities {
	actions: ActionType[];
	max_layers: number;
	max_steps: number;
	max_delay: number;
	layout: {
		rows: [number, number];
		columns: [number, number];
		knobs: [number, number];
		default: [number, number, number];
	};
	led: { modes: string[]; colors: string[] } | null;
	read_keymap: boolean;
	packet_preview: boolean;
}

export interface ModelInfo {
	id: string;
	name: string;
	capabilities: Capabilities;
}

export interface DriverInfo {
	id: string;
	name: string;
	models: ModelInfo[];
}

export interface Catalog {
	drivers: DriverInfo[];
	modifiers: string[];
	keys: string[];
	media: string[];
	software_slots: number;
	platform: 'macos' | 'windows' | 'linux';
	shell: string;
}

export interface AppStateDto {
	profiles: Profiles;
	/** What each config last wrote to its device (by config id), if the device still holds it. */
	applied: Record<string, Mapping>;
	warning: string | null;
	settings: Settings;
	catalog: Catalog;
}

export interface Device {
	driver: string;
	name: string;
	path: string;
	vendor_id: number;
	product_id: number;
	models: string[];
}

export const usbId = (d: Device): string =>
	`${d.vendor_id.toString(16).padStart(4, '0')}:${d.product_id.toString(16).padStart(4, '0')}`;

export interface Fired {
	layer: number;
	slot: string;
	at: number;
}

export interface AgentStatus {
	enabled: boolean;
	available: boolean;
	note: string | null;
	actions: number;
	last: Fired | null;
	problem: string | null;
}

/** Identifies one physical device (crates/core/src/profile.rs). */
export interface DeviceRef {
	vendor_id: number;
	product_id: number;
	serial?: string;
	index: number;
}

/** An attached device, with how a config binds to it. */
export interface Attached extends Device {
	bind: DeviceRef;
	key: string;
}

export interface DeviceConfig {
	id: string;
	name?: string;
	bind: DeviceRef | null;
	keymap: Mapping;
}

export interface Profile {
	id: string;
	name: string;
	configs: DeviceConfig[];
}

export interface Profiles {
	/** The profile last applied. */
	active: string | null;
	profiles: Profile[];
}

export interface Settings {
	agent: boolean;
	tray: boolean;
}

/** serde's encoding of Rust's Result. */
export type Outcome<T> = { Ok: T } | { Err: string };

export interface Status {
	devices: Attached[];
	/** profile id → config id → device key it would be written to, or why not. */
	targets: Record<string, Record<string, Outcome<string>>>;
	agent: AgentStatus;
	watching: boolean;
}

export interface WriteResult {
	config: string;
	hardware: number;
	software: number;
	error: string | null;
}

export type KeypadEvent =
	| { kind: 'media'; code: number }
	| { kind: 'wheel'; delta: number }
	| { kind: 'move'; axis: 'x' | 'y'; delta: number }
	| { kind: 'button'; button: MouseButtonName }
	| { kind: 'key'; mods: number; code: number };

export const api = {
	getState: () => invoke<AppStateDto>('get_state'),
	checkKeymap: (keymap: Mapping) => invoke<Record<string, string>>('check_keymap', { keymap }),
	saveConfig: (profile: string, config: string, keymap: Mapping) =>
		invoke<Record<string, string>>('save_config', { profile, config, keymap }),
	createProfile: (name: string, copyOf: string | null) => invoke<Profile>('create_profile', { name, copyOf }),
	renameProfile: (profile: string, name: string) => invoke<void>('rename_profile', { profile, name }),
	deleteProfile: (profile: string) => invoke<void>('delete_profile', { profile }),
	addConfig: (profile: string, bind: DeviceRef | null, driver: string, model: string) =>
		invoke<DeviceConfig>('add_config', { profile, bind, driver, model }),
	updateConfig: (profile: string, config: string, name: string | null, bind: DeviceRef | null) =>
		invoke<void>('update_config', { profile, config, name, bind }),
	removeConfig: (profile: string, config: string) => invoke<void>('remove_config', { profile, config }),
	preview: (keymap: Mapping) =>
		invoke<{ packets: string[]; hardware: number; software: number }>('preview', { keymap }),
	applyProfile: (profile: string) => invoke<WriteResult[]>('apply_profile', { profile }),
	uploadConfig: (profile: string, config: string) => invoke<WriteResult>('upload_config', { profile, config }),
	starterMapping: (driver: string, model: string) => invoke<Mapping>('starter_mapping', { driver, model }),
	readFromDevice: (driver: string, path: string) => invoke<Mapping>('read_from_device', { driver, path }),
	status: () => invoke<Status>('status'),
	setAgent: (enabled: boolean) => invoke<AgentStatus>('set_agent', { enabled }),
	setTray: (enabled: boolean) => invoke<void>('set_tray', { enabled }),
	testAction: (action: Action) => invoke<void>('test_action', { action }),
	listApps: () => invoke<string[]>('list_apps'),
	importMapping: () => invoke<Mapping | null>('import_mapping'),
	exportMapping: (mapping: Mapping) => invoke<boolean>('export_mapping', { mapping }),
};

export function onKeypadEvent(fn: (e: KeypadEvent) => void): Promise<UnlistenFn> {
	return listen<KeypadEvent>('keypad-event', (e) => fn(e.payload));
}

export function onAgentFired(fn: (f: Fired) => void): Promise<UnlistenFn> {
	return listen<Fired>('agent-fired', (e) => fn(e.payload));
}

/** A profile (or one config) was written, from the window or the tray. */
export function onApplied(
	fn: (profile: string, results: WriteResult[], applied: Record<string, Mapping>) => void,
): Promise<UnlistenFn> {
	return listen<[string, WriteResult[], Record<string, Mapping>]>('applied', (e) => fn(...e.payload));
}

export function onProfilesChanged(fn: (p: Profiles) => void): Promise<UnlistenFn> {
	return listen<Profiles>('profiles-changed', (e) => fn(e.payload));
}

export function onAgentError(fn: (message: string) => void): Promise<UnlistenFn> {
	return listen<string>('agent-error', (e) => fn(e.payload));
}

/** Tauri rejects with the command's error string; anything else, with an Error. */
export function message(e: unknown): string {
	return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}
