import { test } from 'node:test';
import assert from 'node:assert/strict';

import { accordId, codeToKey, deviceView, parseAccord, prettyAccord, riskyActions, setPlatform, slotNames } from '../src/lib/vocab.ts';
import type { Mapping } from '../src/lib/api.ts';

// Key names in HID order from 0x04, as the backend's catalog sends them.
const KEYS = [
	...'abcdefghijklmnopqrstuvwxyz'.split(''),
	...'1234567890'.split(''),
	'enter', 'escape', 'backspace', 'tab', 'space',
];

test('accords parse to HID modifier bits and usage codes', () => {
	assert.deepEqual(parseAccord('ctrl-shift-a', KEYS), [0x03, 0x04]);
	assert.deepEqual(parseAccord('cmd-c', KEYS), [0x08, 0x06]);
	assert.deepEqual(parseAccord('win-c', KEYS), [0x08, 0x06]);
	assert.deepEqual(parseAccord('esc', KEYS), [0, 0x29]);
	assert.deepEqual(parseAccord('<104>', KEYS), [0, 104]);
	assert.deepEqual(parseAccord('ctrl-alt', KEYS), [0x05, 0]);
	assert.equal(parseAccord('hyper-a', KEYS), null);
});

test('right-hand modifiers fold onto the left ones for hit matching', () => {
	assert.equal(accordId('rctrl-a', KEYS), accordId('ctrl-a', KEYS));
	assert.equal(accordId('ctrl', KEYS), null);
});

test('keycap labels follow the platform', () => {
	setPlatform('macos');
	assert.equal(prettyAccord('cmd-shift-4'), '⇧⌘4');
	setPlatform('windows');
	assert.equal(prettyAccord('ctrl-shift-s'), 'Ctrl+Shift+S');
	assert.equal(prettyAccord('win'), 'Win');
});

test('recorder key codes', () => {
	assert.equal(codeToKey('KeyQ'), 'q');
	assert.equal(codeToKey('Digit7'), '7');
	assert.equal(codeToKey('Numpad3'), 'numpad3');
	assert.equal(codeToKey('F13'), 'f13');
	assert.equal(codeToKey('BracketLeft'), 'leftbracket');
	assert.equal(codeToKey('Fn'), null);
});

const mapping = (): Mapping => ({
	driver: 'ch57x',
	model: 'ch57x-1',
	rows: 1,
	columns: 2,
	knobs: 1,
	layers: [
		{
			name: 'Edit',
			slots: {
				b0: { type: 'keys', steps: ['ctrl-c'], label: 'Copy' },
				b1: { type: 'shell', command: 'rm -rf ~/tmp' },
				'k0.press': { type: 'macro', steps: [{ do: 'open', value: 'https://example.com' }] },
			},
		},
	],
});

test('slot names cover buttons then knobs', () => {
	assert.deepEqual(slotNames(mapping()), ['b0', 'b1', 'k0.ccw', 'k0.press', 'k0.cw']);
});

test('labels and layer names never make the keypad look out of date', () => {
	const a = mapping();
	const b = mapping();
	b.layers[0].name = 'Renamed';
	(b.layers[0].slots.b0 as { label?: string }).label = 'Other';
	assert.equal(deviceView(a), deviceView(b));
	b.layers[0].slots.b0 = { type: 'keys', steps: ['ctrl-v'] };
	assert.notEqual(deviceView(a), deviceView(b));
});

test('imports list every command they would run', () => {
	const risky = riskyActions(mapping());
	assert.equal(risky.length, 2);
	assert.match(risky[0], /\$ rm -rf/);
	assert.match(risky[1], /open https:\/\/example.com/);
});
