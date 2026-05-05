import { invoke } from "@tauri-apps/api/core";
import { Entry } from "./types";

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
}) => invoke<Entry>("add_entry", payload);

export const updateEntry = (payload: {
  id: string;
  name: string;
  username?: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
}) => invoke<void>("update_entry", payload);

export const deleteEntry = (id: string) =>
  invoke<void>("delete_entry", { id });

export const enableAutostart = () =>
  invoke<void>("enable_autostart");

export const disableAutostart = () =>
  invoke<void>("disable_autostart");

export const isAutostartEnabled = () =>
  invoke<boolean>("is_autostart_enabled");