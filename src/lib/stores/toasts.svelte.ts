export type ToastKind = 'ok' | 'err' | 'info';

export interface ToastItem {
	id: number;
	title: string;
	sub: string;
	kind: ToastKind;
	leaving: boolean;
}

class Toasts {
	items = $state<ToastItem[]>([]);
	#next = 0;

	show(title: string, sub = '', kind: ToastKind = 'info'): void {
		const id = ++this.#next;
		this.items.push({ id, title, sub, kind, leaving: false });
		setTimeout(
			() => {
				const item = this.items.find((x) => x.id === id);
				if (item) item.leaving = true;
				setTimeout(() => (this.items = this.items.filter((x) => x.id !== id)), 300);
			},
			kind === 'err' ? 6000 : 3500,
		);
	}
}

export const toasts = new Toasts();
export const toast = (title: string, sub = '', kind: ToastKind = 'info'): void => toasts.show(title, sub, kind);
