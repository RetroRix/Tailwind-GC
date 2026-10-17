// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// App identity + build stamp for the About dialog. The version comes from package.json and the commit id
// is captured at build time by Vite `define` (see vite.config.js) — no manual edit per build.

export const APP_NAME = 'Tailwind Ground Control';
export const APP_TAGLINE = 'Go far. Get home.';
export const APP_VERSION = __APP_VERSION__;
export const GIT_COMMIT = __GIT_COMMIT__;
export const BUILD_DATE = __BUILD_DATE__;
export const COPYRIGHT = '© 2026 Marc Hoffmann (b14ckyy) · Tailwind modifications © 2026 RetroRix';
export const LICENSE = 'GPL-3.0-or-later';
export const REPO_URL = 'https://github.com/RetroRix/Tailwind-GC';
/** Upstream project Tailwind GC is based on (credit + original docs). */
export const UPSTREAM_URL = 'https://github.com/b14ckyy/Kite-GC';
export const DOCS_URL = 'https://github.com/RetroRix/Tailwind-GC';
export const PRIVACY_URL = '';
export const DISCORD_URL = '';
/** Store listings for the mobile builds — empty until the listings exist; the update dialog then falls
 *  back to the GitHub release page. */
export const PLAY_STORE_URL = '';
export const APP_STORE_URL = '';
