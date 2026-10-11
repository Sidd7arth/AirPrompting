/**
 * AirPrompt's product identity, in one place (spec §19).
 *
 * Every name the UI uses to identify the product lives here: the display name,
 * the identifier, the browser-storage key prefix, and the event channel the
 * Rust core pushes state through. Renaming the product is one edit in this file
 * plus one in `src-tauri/core/src/identity.rs` — the two must stay equal.
 *
 * Nothing third-party belongs here: React, Vite, Tauri, whisper.cpp and
 * FluidAudio keep their own names (spec §26).
 */

/** Product name, as shown to the user in every surface. */
export const APP_NAME = "AirPrompt";

/** Reverse-DNS identifier, matching `identity::APP_IDENTIFIER` in Rust. */
export const APP_IDENTIFIER = "com.airprompt.app";

/** Channel carrying core events to the UI; mirrors `identity::EVENT_CHANNEL`. */
export const EVENT_CHANNEL = "airprompt://event";

/** Prefix for this product's own browser-storage keys. */
export const STORAGE_PREFIX = "airprompt";

/**
 * Storage-key prefix used by an earlier build of the product.
 *
 * Migration identifier (spec §33): a preference written under the old prefix is
 * still read, so nobody loses their theme on upgrade. It is never written.
 */
export const LEGACY_STORAGE_PREFIX = "jarvis";

/** Browser-storage key holding the theme preference. */
export const THEME_KEY = `${STORAGE_PREFIX}.theme`;

/**
 * Persist the theme under the AirPrompt key.
 *
 * The earlier key is left where it is rather than deleted — it is a single
 * string, and "never delete the old data" is a rule worth holding to without
 * exceptions.
 */
export function setStoredTheme(theme: string): void {
  localStorage.setItem(THEME_KEY, theme);
}
