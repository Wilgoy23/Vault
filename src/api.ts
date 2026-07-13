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
}) => invoke<void>("update_entry", payload);

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

// The save/open dialogs run on the Rust side so file paths never transit IPC.
// Both resolve to false when the user cancels the dialog.
export const exportVault = () =>
  invoke<boolean>("export_vault");

export const importVault = () =>
  invoke<boolean>("import_vault");

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