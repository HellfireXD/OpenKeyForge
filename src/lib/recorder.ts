// Svelte action for a Record button: while on, every key pressed in the
// window becomes an accord ("ctrl-shift-a"); a modifier pressed and released
// alone becomes a lone-modifier accord ("ctrl").
import { t } from './i18n.js';
import { codeToKey, MOD_CODES, sortMods } from './vocab.js';
import { studio } from './stores/studio.svelte.js';
import { toast } from './stores/toasts.svelte.js';

let active: { node: HTMLElement; stop: () => void } | null = null;

export function stopRecording(): void {
	active?.stop();
}

export function recorder(node: HTMLButtonElement, onAccord: (accord: string) => void) {
	let emit = onAccord;
	const text = () => node.querySelector('span:last-child');

	const start = () => {
		if (active) {
			const same = active.node === node;
			active.stop();
			if (same) return;
		}
		const held = new Set<string>();
		let usedKey = false;
		const down = (e: KeyboardEvent) => {
			e.preventDefault();
			e.stopPropagation();
			if (e.repeat) return;
			if (MOD_CODES[e.code]) {
				held.add(MOD_CODES[e.code]);
				usedKey = false;
				return;
			}
			const k = codeToKey(e.code);
			if (!k) {
				toast(t('keys.cant'), e.code, 'err');
				return;
			}
			usedKey = true;
			emit([...sortMods(held), k].join('-'));
		};
		const up = (e: KeyboardEvent) => {
			e.preventDefault();
			const m = MOD_CODES[e.code];
			if (!m) return;
			if (!usedKey && held.size) emit(sortMods(held).join('-'));
			held.delete(m);
			if (!held.size) usedKey = false;
		};
		window.addEventListener('keydown', down, true);
		window.addEventListener('keyup', up, true);
		node.classList.add('on');
		const tx = text();
		if (tx) tx.textContent = t('keys.recStop');
		studio.recording = true;
		active = {
			node,
			stop() {
				window.removeEventListener('keydown', down, true);
				window.removeEventListener('keyup', up, true);
				node.classList.remove('on');
				const tx = text();
				if (tx) tx.textContent = t('keys.rec');
				studio.recording = false;
				active = null;
			},
		};
	};

	node.addEventListener('click', start);
	return {
		update(fn: (accord: string) => void) {
			emit = fn;
		},
		destroy() {
			if (active?.node === node) active.stop();
			node.removeEventListener('click', start);
		},
	};
}
