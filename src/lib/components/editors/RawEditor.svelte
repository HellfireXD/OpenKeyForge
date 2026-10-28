<script lang="ts">
	import type { Action } from '../../api.js';
	import { t } from '../../i18n.js';

	let { a }: { a: Extract<Action, { type: 'raw' }> } = $props();
</script>

<div>
	<div class="sec-h">{t('raw.title')}</div>
	<div class="group">
		<div class="stack">
			<input class="field mono" value={a.expr} placeholder={'{delay(100)}ctrl-a,ctrl-c'} oninput={(e) => (a.expr = e.currentTarget.value)} />
		</div>
	</div>
	<div class="footer">
		{t('raw.footer')}
		<code>ctrl-a,b</code> <code>{'{delay(200)}left,right'}</code> <code>shift-wheel(3)</code>
		<code>click+rclick</code> <code>consumer(0x1b6)</code> <code>&lt;104&gt;</code>
	</div>
</div>
