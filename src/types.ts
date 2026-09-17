export interface Folder {
  id: string;
  name: string;
}

/** Only logins exist today; the backend carries the field for future item types. */
export type EntryKind = "login";

export interface PastPassword {
  password: string;
  /** Unix seconds */
  replaced_at: number;
}

export interface Entry {
  id: string;
  name: string;
  kind?: EntryKind;
  username?: string;
  /** May be empty — an entry needs either an email or a username. */
  email: string;
  password: string;
  url?: string;
  notes?: string;
  folder_id?: string;
  totp_secret?: string;
  created_at: number;
  updated_at: number;
  /** When the password itself last changed. Absent on entries saved before
   *  this was tracked, where `updated_at` is the only date available. */
  password_changed_at?: number;
  /** Superseded passwords, oldest first. */
  password_history?: PastPassword[];
  last_used_at?: number;
  use_count?: number;
}
