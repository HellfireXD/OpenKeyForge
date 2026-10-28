// Icons, keycap labels and the small parsers the editor needs, ported from
// macropad-studio's ui/app.js and ui/ch57x.js (MIT, elniakim).
import type { Action, ActionType, Layer, Mapping } from './api.js';
import { t } from './i18n.js';

/* SF Symbols-style line icons. Static strings, safe for {@html}. */
const P: Record<string, string> = {
	keys: '<rect x="2.5" y="6" width="19" height="12" rx="2.5"/><path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M8 14h8"/>',
	media: '<path d="M9 18V5.5l11-2V16"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="17.5" cy="16" r="2.5"/>',
	mouse: '<path d="M5 3.5l6.5 16 2.3-6.7 6.7-2.3z"/>',
	raw: '<path d="M8 4H7a2 2 0 0 0-2 2v4a2 2 0 0 1-2 2 2 2 0 0 1 2 2v4a2 2 0 0 0 2 2h1M16 4h1a2 2 0 0 1 2 2v4a2 2 0 0 0 2 2 2 2 0 0 0-2 2v4a2 2 0 0 1-2 2h-1"/>',
	text: '<path d="M5 7V5h14v2M12 5v14M9 19h6"/>',
	open: '<rect x="3.5" y="3.5" width="17" height="17" rx="4"/><path d="M9.5 14.5l5-5M10.5 9.5h4v4"/>',
	shell: '<path d="M5 7l5 5-5 5M12.5 17H19"/>',
	macro: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r=".6"/><circle cx="4.5" cy="12" r=".6"/><circle cx="4.5" cy="18" r=".6"/>',
	speaker: '<path d="M4 9.5h3l4.5-4v13l-4.5-4H4z"/>',
	wave1: '<path d="M15 9.5a3.5 3.5 0 0 1 0 5"/>',
	wave2: '<path d="M17.5 7a7 7 0 0 1 0 10"/>',
	mutex: '<path d="M15.5 9.5l5 5M20.5 9.5l-5 5"/>',
	playpause: '<path d="M4 6l7 6-7 6z"/><path d="M15 6v12M19.5 6v12"/>',
	next: '<path d="M4 6l7 6-7 6zM11 6l7 6-7 6zM20 6v12"/>',
	prev: '<path d="M20 6l-7 6 7 6zM13 6l-7 6 7 6zM4 6v12"/>',
	stop: '<rect x="6" y="6" width="12" height="12" rx="1.5"/>',
	sunup: '<circle cx="12" cy="12" r="3.5"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4L7 17M17 7l1.4-1.4"/>',
	sundown: '<circle cx="12" cy="12" r="3.5"/><path d="M12 5.5v.01M12 18.5v.01M5.5 12h.01M18.5 12h.01M7.4 7.4h.01M16.6 16.6h.01M7.4 16.6h.01M16.6 7.4h.01"/>',
	calc: '<rect x="5" y="3" width="14" height="18" rx="2.5"/><path d="M8.5 7h7M8.5 11.5h.01M12 11.5h.01M15.5 11.5h.01M8.5 15h.01M12 15h.01M15.5 15h.01M8.5 18h.01M12 18h.01M15.5 18h.01"/>',
	lock: '<rect x="5" y="10.5" width="14" height="10" rx="2.5"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
	star: '<path d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.8l-5.2 2.8 1-5.8-4.3-4.1 5.9-.8z"/>',
	home: '<path d="M4 11l8-7 8 7M6 9.5V20h12V9.5"/>',
	back: '<path d="M15 5l-7 7 7 7"/>',
	fwd: '<path d="M9 5l7 7-7 7"/>',
	check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
	warn: '<path d="M12 4l9 16H3z"/><path d="M12 10v4M12 17h.01"/>',
	info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>',
	ccw: '<path d="M4.5 10a8 8 0 1 1 1.6 6.5"/><path d="M4 5v5h5"/>',
	cw: '<path d="M19.5 10a8 8 0 1 0-1.6 6.5"/><path d="M20 5v5h-5"/>',
	dots: '<circle cx="5" cy="12" r=".8"/><circle cx="12" cy="12" r=".8"/><circle cx="19" cy="12" r=".8"/>',
};

export const icon = (...names: string[]): string =>
	`<svg class="i" viewBox="0 0 24 24">${names.map((n) => P[n] || '').join('')}</svg>`;

export const DEV: ReadonlySet<ActionType> = new Set(['keys', 'media', 'mouse', 'raw']);
export const TYPE_LIST: ActionType[] = ['keys', 'media', 'mouse', 'raw', 'text', 'open', 'shell', 'macro'];
export const kindOf = (ty: ActionType): 'dev' | 'agt' => (DEV.has(ty) ? 'dev' : 'agt');

export const MEDIA_ICON: Record<string, string[]> = {
	play: ['playpause'],
	previous: ['prev'],
	prev: ['prev'],
	next: ['next'],
	stop: ['stop'],
	volumedown: ['speaker', 'wave1'],
	mute: ['speaker', 'mutex'],
	volumeup: ['speaker', 'wave1', 'wave2'],
	screenbrightnessdown: ['sundown'],
	screenbrightnessup: ['sunup'],
	calculator: ['calc'],
	screenlock: ['lock'],
	favorites: ['star'],
	webpagehome: ['home'],
	webpageback: ['back'],
	webpageforward: ['fwd'],
};

export const LED_HEX: Record<string, string> = {
	white: '#f5f5f7',
	red: '#ff3b30',
	orange: '#ff9500',
	yellow: '#ffcc00',
	green: '#34c759',
	cyan: '#32ade6',
	blue: '#007aff',
	purple: '#af52de',
};

export const KNOB_ACTIONS = ['ccw', 'press', 'cw'] as const;

export const num = (v: unknown, d = 0): number => {
	const n = Number(v);
	return Number.isFinite(n) ? n : d;
};

/* ---------- platform-aware keycap text ---------- */

let mac = false;
export function setPlatform(platform: string): void {
	mac = platform === 'macos';
}
export const isMac = (): boolean => mac;

const MAC_SYM: Record<string, string> = {
	ctrl: '⌃', alt: '⌥', opt: '⌥', shift: '⇧', cmd: '⌘', win: '⌘',
	rctrl: 'R⌃', ralt: 'R⌥', ropt: 'R⌥', rshift: 'R⇧', rcmd: 'R⌘', rwin: 'R⌘',
};
const PC_SYM: Record<string, string> = {
	ctrl: 'Ctrl', alt: 'Alt', opt: 'Alt', shift: 'Shift', cmd: 'Win', win: 'Win',
	rctrl: 'RCtrl', ralt: 'RAlt', ropt: 'RAlt', rshift: 'RShift', rcmd: 'RWin', rwin: 'RWin',
};
export const modSym = (m: string): string => (mac ? MAC_SYM : PC_SYM)[m] ?? m;
export const MOD_ORDER = ['ctrl', 'rctrl', 'alt', 'ralt', 'shift', 'rshift', 'cmd', 'rcmd', 'win', 'rwin'];
const KEY_SYM: Record<string, string> = {
	enter: '↩', escape: 'esc', esc: 'esc', backspace: '⌫', delete: '⌦', tab: '⇥', space: 'Space', capslock: '⇪',
	left: '←', right: '→', up: '↑', down: '↓', pageup: '⇞', pagedown: '⇟', home: '↖', end: '↘',
	minus: '-', equal: '=', leftbracket: '[', rightbracket: ']', backslash: '\\', semicolon: ';', quote: "'",
	grave: '`', comma: ',', dot: '.', slash: '/', numpadenter: '⌤',
};

export const sortMods = (mods: Iterable<string>): string[] =>
	[...mods].sort((x, y) => MOD_ORDER.indexOf(x) - MOD_ORDER.indexOf(y));

export function prettyAccord(a: string): string {
	const parts = String(a).toLowerCase().split('-');
	const key = parts.pop() ?? '';
	const mods = parts;
	let keyTxt: string;
	if (MAC_SYM[key]) {
		mods.push(key);
		keyTxt = '';
	} else {
		keyTxt =
			KEY_SYM[key] ??
			(key.length === 1
				? key.toUpperCase()
				: key.replace(/^f(\d+)$/, 'F$1').replace(/^numpad/, 'num').replace(/^<(\d+)>$/, '#$1'));
	}
	const syms = sortMods(mods).map(modSym);
	if (mac) return syms.join('') + keyTxt;
	return [...syms, keyTxt].filter(Boolean).join('+');
}

export const mediaName = (c: string | number): string =>
	typeof c === 'string' && MEDIA_ICON[c] ? t('media.' + (c === 'prev' ? 'previous' : c)) : t('auto.media', { c });

export function autoLabel(a: Action | undefined): string {
	if (!a) return '';
	switch (a.type) {
		case 'keys':
			return (a.steps || []).map(prettyAccord).join(' ') || t('auto.keys');
		case 'media':
			return mediaName(a.code);
		case 'mouse': {
			const b = (a.buttons || []).map((x) => t('btn.' + x)).join('+');
			const mod = a.modifier ? modSym(a.modifier) + (mac ? '' : '+') : '';
			if (a.action === 'click') return mod + t('auto.click', { b });
			if (a.action === 'wheel') {
				const d = num(a.delta);
				return mod + t('auto.scroll', { d: (d > 0 ? t('auto.up') : t('auto.down')) + (Math.abs(d) !== 1 ? ' ' + Math.abs(d) : '') });
			}
			if (a.action === 'move') return mod + t('auto.move', { x: num(a.dx), y: num(a.dy) });
			return mod + t('auto.drag', { b });
		}
		case 'raw':
			return a.expr || t('type.raw');
		case 'text':
			return a.text ? `“${a.text.replace(/\n/g, ' ')}”` : t('type.text');
		case 'open': {
			const s = a.target || '';
			if (!s) return t('type.open');
			return s.includes('://')
				? s.replace(/^\w+:\/\/(www\.)?/, '').split('/')[0]
				: (s.split(/[\\/]/).pop() ?? s).replace(/\.(app|exe|desktop)$/i, '');
		}
		case 'shell':
			return a.command || t('type.shell');
		case 'macro':
			return t('auto.macro', { n: (a.steps || []).length });
	}
}

export const label = (a: Action | undefined): string => (a && a.label) || autoLabel(a);

export function legendIcon(a: Action): string {
	if (a.type === 'media' && typeof a.code === 'string' && MEDIA_ICON[a.code]) return icon(...MEDIA_ICON[a.code]);
	if (a.type !== 'keys' && a.type !== 'media') return icon(a.type);
	return '';
}

export function slotTitle(n: string): string {
	if (n.startsWith('b')) return t('slot.button', { n: +n.slice(1) + 1 });
	const [k, act] = n.slice(1).split('.');
	return t('slot.knob', { n: +k + 1, a: t('knob.' + act) });
}

export const layerName = (l: Layer, i: number): string => l.name || t('layer', { n: i + 1 });

export function slotNames(m: Mapping): string[] {
	const names = Array.from({ length: num(m.rows) * num(m.columns) }, (_, i) => 'b' + i);
	for (let k = 0; k < num(m.knobs); k++) for (const a of KNOB_ACTIONS) names.push(`k${k}.${a}`);
	return names;
}

export function defaultAction(ty: ActionType): Action {
	return {
		keys: { type: 'keys', steps: [], delay: 0 },
		media: { type: 'media', code: 'play' },
		mouse: { type: 'mouse', action: 'click', buttons: ['left'], modifier: '' },
		raw: { type: 'raw', expr: '' },
		text: { type: 'text', text: '' },
		open: { type: 'open', target: '' },
		shell: { type: 'shell', command: '' },
		macro: { type: 'macro', steps: [] },
	}[ty] as Action;
}

export function ledColor(led: string | undefined): string {
	const [mode, color] = String(led || 'off').split(' ');
	return mode === 'off' ? 'transparent' : LED_HEX[color || 'white'] || 'transparent';
}

/** What the keypad holds: everything but labels and layer names. */
export function deviceView(m: Mapping | null): string | null {
	if (!m) return null;
	return JSON.stringify({
		driver: m.driver,
		model: m.model,
		rows: m.rows,
		columns: m.columns,
		knobs: m.knobs,
		layers: m.layers.map((l) => ({
			led: l.led,
			slots: Object.fromEntries(
				Object.entries(l.slots)
					.sort()
					.map(([k, v]) => {
						const { label: _label, ...rest } = v;
						return [k, rest];
					}),
			),
		})),
	});
}

/** Shell and open actions in a mapping, shown before importing it. */
export function riskyActions(m: Mapping): string[] {
	const out: string[] = [];
	const clip = (v: string) => String(v).replace(/\s+/g, ' ').slice(0, 160);
	m.layers.forEach((l, li) =>
		Object.entries(l.slots).forEach(([slot, a]) => {
			const where = `${li + 1}:${slot}`;
			if (a.type === 'shell') out.push(`${where}  $ ${clip(a.command)}`);
			if (a.type === 'open') out.push(`${where}  open ${clip(a.target)}`);
			if (a.type === 'macro')
				for (const st of a.steps) {
					if (st.do === 'shell') out.push(`${where}  $ ${clip(st.value)}`);
					if (st.do === 'open') out.push(`${where}  open ${clip(st.value)}`);
				}
		}),
	);
	return out;
}

/* ---------- key recorder + live test ---------- */

const CODE_MAP: Record<string, string> = {
	Enter: 'enter', NumpadEnter: 'numpadenter', Escape: 'escape', Backspace: 'backspace', Tab: 'tab', Space: 'space',
	Minus: 'minus', Equal: 'equal', BracketLeft: 'leftbracket', BracketRight: 'rightbracket', Backslash: 'backslash',
	Semicolon: 'semicolon', Quote: 'quote', Backquote: 'grave', Comma: 'comma', Period: 'dot', Slash: 'slash',
	CapsLock: 'capslock', PrintScreen: 'printscreen', ScrollLock: 'scrolllock', Pause: 'pause', Insert: 'insert',
	Home: 'home', PageUp: 'pageup', Delete: 'delete', End: 'end', PageDown: 'pagedown',
	ArrowRight: 'right', ArrowLeft: 'left', ArrowDown: 'down', ArrowUp: 'up', NumLock: 'numlock',
	NumpadDivide: 'numpadslash', NumpadMultiply: 'numpadasterisk', NumpadSubtract: 'numpadminus', NumpadAdd: 'numpadplus',
	NumpadDecimal: 'numpaddot', NumpadEqual: 'numpadequal', IntlBackslash: 'nonusbackslash', ContextMenu: 'application',
};
export const MOD_CODES: Record<string, string> = {
	ControlLeft: 'ctrl', ControlRight: 'rctrl', ShiftLeft: 'shift', ShiftRight: 'rshift',
	AltLeft: 'alt', AltRight: 'ralt', MetaLeft: 'win', MetaRight: 'rwin',
};
export const MEDIA_CODES: Record<string, string> = {
	AudioVolumeUp: 'volumeup', AudioVolumeDown: 'volumedown', AudioVolumeMute: 'mute',
	MediaPlayPause: 'play', MediaTrackNext: 'next', MediaTrackPrevious: 'previous', MediaStop: 'stop',
};

export function codeToKey(code: string): string | null {
	if (CODE_MAP[code]) return CODE_MAP[code];
	let m: RegExpMatchArray | null;
	if ((m = code.match(/^Key([A-Z])$/))) return m[1].toLowerCase();
	if ((m = code.match(/^Digit(\d)$/))) return m[1];
	if ((m = code.match(/^Numpad(\d)$/))) return 'numpad' + m[1];
	if ((m = code.match(/^F(\d+)$/))) return 'f' + m[1];
	return null;
}

const MOD_BITS: Record<string, number> = {
	ctrl: 0x01, shift: 0x02, alt: 0x04, opt: 0x04, win: 0x08, cmd: 0x08,
	rctrl: 0x10, rshift: 0x20, ralt: 0x40, ropt: 0x40, rwin: 0x80, rcmd: 0x80,
};
const KEY_ALIASES: Record<string, string> = { esc: 'escape', macbrightnessdown: 'scrolllock', macbrightnessup: 'pause' };

/** (modifier bits, HID code) of an accord such as "ctrl-shift-a", given the key names in HID order. */
export function parseAccord(s: string, keys: string[]): [number, number] | null {
	const parts = s.trim().toLowerCase().split('-');
	let mods = 0;
	for (const p of parts.slice(0, -1)) {
		if (!(p in MOD_BITS)) return null;
		mods |= MOD_BITS[p];
	}
	const last = parts[parts.length - 1];
	const custom = last.match(/^<(\d+)>$/);
	if (custom) return [mods, +custom[1]];
	const i = keys.indexOf(KEY_ALIASES[last] ?? last);
	if (i >= 0) return [mods, 0x04 + i];
	if (last in MOD_BITS) return [mods | MOD_BITS[last], 0];
	return null;
}

/** "mods:code" with right-hand modifiers folded onto the left ones, or null. */
export function accordId(step: string | undefined, keys: string[]): string | null {
	const p = step ? parseAccord(step, keys) : null;
	return p && p[1] ? `${(p[0] | (p[0] >> 4)) & 0x0f}:${p[1]}` : null;
}

export function mediaCodeOf(c: string | number, media: Record<string, number>): number | null {
	if (typeof c === 'number') return c;
	if (c in media) return media[c];
	return /^(0x)?[0-9a-f]+$/i.test(c) ? parseInt(c, c.toLowerCase().startsWith('0x') ? 16 : 10) : null;
}

/** HID Consumer codes of the named media keys (crates/ch57x MediaCode). */
export const MEDIA_USAGE: Record<string, number> = {
	screenbrightnessup: 0x6f, screenbrightnessdown: 0x70, next: 0xb5, previous: 0xb6, prev: 0xb6, stop: 0xb7,
	play: 0xcd, mute: 0xe2, volumeup: 0xe9, volumedown: 0xea, favorites: 0x182, calculator: 0x192,
	screenlock: 0x19e, webpagehome: 0x223, webpageback: 0x224, webpageforward: 0x225,
};
