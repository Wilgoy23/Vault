import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { Entry, Folder } from "./types";

export const vaultExists = () =>
  invoke<boolean>("vault_exists");

export const createVault = (password: string) =>
  invoke<void>("create_vault", { password });

export const unlock = (password: string) =>
  invoke<void>("unlock", { password });

export const lock = () =>
  invoke<void>("lock");

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
  folder_id?: string;
}) => invoke<Entry>("add_entry", payload);

export const updateEntry = (payload: {
  id: string;
  name: string;
  username?: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
  folder_id?: string;
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

export const exportVault = async (): Promise<boolean> => {
  const destPath = await save({
    filters: [{ name: "Vault Backup", extensions: ["enc"] }],
    defaultPath: "vault-backup.enc",
  });
  if (!destPath) return false;
  await invoke<void>("export_vault", { destPath });
  return true;
};

export const importVault = async (): Promise<boolean> => {
  const srcPath = await open({
    filters: [{ name: "Vault Backup", extensions: ["enc"] }],
    multiple: false,
    directory: false,
  });
  if (!srcPath) return false;
  await invoke<void>("import_vault", { srcPath });
  return true;
};

export const enableAutostart = () =>
  invoke<void>("enable_autostart");

export const disableAutostart = () =>
  invoke<void>("disable_autostart");

export const isAutostartEnabled = () =>
  invoke<boolean>("is_autostart_enabled");

export const getOverlayShortcut = () =>
  invoke<string>("get_overlay_shortcut");

export const setOverlayShortcut = (shortcutStr: string) =>
  invoke<void>("set_overlay_shortcut", { shortcutStr });