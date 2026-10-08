import { invoke } from "@tauri-apps/api/core";
import { Entry, Folder } from "./types";

export const vaultExists = () =>
  invoke<boolean>("vault_exists");

export const createVault = (password: string) =>
  invoke<void>("create_vault", { password });

export const unlock = (password: string) =>
  invoke<void>("unlock", { password });

export const lock = () =>
  invoke<void>("lock");

export const changeMasterPassword = (currentPassword: string, newPassword: string) =>
  invoke<void>("change_master_password", { currentPassword, newPassword });

/** Windows Hello quick unlock. Nothing is stored on disk: the wrapped key
 *  lives in the backend process only, so a cold start needs the master
 *  password. Every one of these is a no-op returning false off Windows. */
export const quickUnlockAvailable = () =>
  invoke<boolean>("quick_unlock_available");

export const quickUnlockArmed = () =>
  invoke<boolean>("quick_unlock_armed");

/** Prompts for the Hello gesture once, while the vault is open. */
export const armQuickUnlock = () =>
  invoke<void>("arm_quick_unlock");

export const quickUnlock = () =>
  invoke<void>("quick_unlock");

export const disarmQuickUnlock = () =>
  invoke<void>("disarm_quick_unlock");

/** iOS Password AutoFill. `supported` is false off iOS, and on builds that
 *  lack the App Group the extension reads from. Enabling needs the vault
 *  open; after that the backend keeps it current on every unlock and save. */
export const autofillStatus = () =>
  invoke<{ supported: boolean; enabled: boolean }>("autofill_status");

export const setAutofillEnabled = (enabled: boolean) =>
  invoke<void>("set_autofill_enabled", { enabled });

/** Lock the vault when the screen locks or the machine sleeps. */
export const setLockOnSystemEvents = (enabled: boolean) =>
  invoke<void>("set_lock_on_system_events", { enabled });

export const isUnlocked = () =>
  invoke<boolean>("is_unlocked");

export const listEntries = () =>
  invoke<Entry[]>("list_entries");

export const addEntry = (payload: {
  name: string;
  username?: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
  folderId?: string;
  totpSecret?: string;
}) => invoke<Entry>("add_entry", payload);

export const updateEntry = (payload: {
  id: string;
  name: string;
  username?: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
  folderId?: string;
  totpSecret?: string;
}) => invoke<Entry>("update_entry", payload);

export const listFolders = () =>
  invoke<Folder[]>("list_folders");

export const addFolder = (name: string) =>
  invoke<Folder>("add_folder", { name });

export const renameFolder = (id: string, name: string) =>
  invoke<void>("rename_folder", { id, name });

export const deleteFolder = (id: string) =>
  invoke<void>("delete_folder", { id });

export const deleteEntry = (id: string) =>
  invoke<void>("delete_entry", { id });

// Stamps last_used_at / use_count on the entry so the overlay can order
// by recency. Fire-and-forget from copy handlers.
export const markEntryUsed = (id: string) =>
  invoke<void>("mark_entry_used", { id });

// The save/open dialogs run on the Rust side so file paths never transit IPC.
// Both resolve to false when the user cancels the dialog.

// A native picker takes over the screen on mobile, which hides the webview.
// That looks exactly like the app being backgrounded, so the lock-on-background
// watcher checks this before locking — otherwise choosing a file to import
// would lock the vault out from under the import.
let openDialogs = 0;

const withNativeDialog = async <T>(run: () => Promise<T>): Promise<T> => {
  openDialogs++;
  try {
    return await run();
  } finally {
    openDialogs--;
  }
};

export const nativeDialogOpen = () => openDialogs > 0;

export const exportVault = () =>
  withNativeDialog(() => invoke<boolean>("export_vault"));

export const importVault = () =>
  withNativeDialog(() => invoke<boolean>("import_vault"));

export interface CsvImportReport {
  imported: number;
  skipped: number;
}

// Resolves to null when the user cancels the file dialog.
export const importCsv = () =>
  withNativeDialog(() => invoke<CsvImportReport | null>("import_csv"));

/** Sync folder (desktop only; `supported` is false on mobile). The vault file
 *  lives in a folder OneDrive, Dropbox, iCloud Drive or Syncthing keeps in
 *  step across devices, and the backend merges other devices' changes in.
 *  It emits "vault-changed" when they arrive. */
export interface SyncStatus {
  supported: boolean;
  folder: string | null;
  /** Configured, but the folder isn't there right now */
  missing: boolean;
}

export const syncStatus = () =>
  invoke<SyncStatus>("sync_status");

/** "needs_password": the folder already holds another device's vault; pass
 *  that vault's master password to joinSyncFolder to merge into it. */
export const chooseSyncFolder = () =>
  withNativeDialog(() => invoke<"cancelled" | "moved" | "needs_password">("choose_sync_folder"));

export const joinSyncFolder = (password: string) =>
  invoke<void>("join_sync_folder", { password });

/** For a device with no vault yet. Resolves to false on cancel. */
export const openSyncedVault = () =>
  withNativeDialog(() => invoke<boolean>("open_synced_vault"));

export const stopSync = () =>
  invoke<void>("stop_sync");

export const enableAutostart = () =>
  invoke<void>("enable_autostart");

export const disableAutostart = () =>
  invoke<void>("disable_autostart");

export const isAutostartEnabled = () =>
  invoke<boolean>("is_autostart_enabled");

export const writeClipboardText = (text: string) =>
  invoke<void>("write_clipboard_text", { text });

export const scheduleClipboardClear = (seconds: number) =>
  invoke<void>("schedule_clipboard_clear", { seconds });

export const getOverlayShortcut = () =>
  invoke<string>("get_overlay_shortcut");

export const setOverlayShortcut = (shortcutStr: string) =>
  invoke<void>("set_overlay_shortcut", { shortcutStr });