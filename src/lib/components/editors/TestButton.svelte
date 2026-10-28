<script lang="ts">
	import { api, message, type Action } from '../../api.js';
	import { t } from '../../i18n.js';
	import { toast } from '../../stores/toasts.svelte.js';

	let { a }: { a: Action } = $props();

	async function run() {
		try {
			await api.testAction($state.snapshot(a) as Action);
			toast(t('test.toast'), t('test.toastSub'));
		} catch (e) {
			toast(t('test.fail'), message(e), 'err');
		}
	}
</script>

<div>
	<button class="btn lg" onclick={run}>{t('test')}</button>
	<div class="footer" style="margin-left:2px">{t('test.hint')}</div>
</div>
