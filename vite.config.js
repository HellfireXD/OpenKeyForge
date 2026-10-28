import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
	plugins: [
		sveltekit({
			preprocess: vitePreprocess(),
			// Tauri has no Node.js server for SSR, so adapter-static with an
			// index.html fallback puts the site in SPA mode.
			// See: https://v2.tauri.app/start/frontend/sveltekit/
			adapter: adapter({
				fallback: 'index.html',
			}),
		}),
	],

	// Vite options tailored for Tauri development, only applied in `tauri dev` or `tauri build`:
	// 1. don't obscure Rust errors
	clearScreen: false,
	// 2. Tauri expects a fixed port; fail if it isn't available
	server: {
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host
			? {
					protocol: 'ws',
					host,
					port: 1421,
				}
			: undefined,
		watch: {
			// 3. don't watch `src-tauri`
			ignored: ['**/src-tauri/**'],
		},
	},
}));
