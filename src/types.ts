export interface Folder {
  id: string;
  name: string;
}

export interface Entry {
  id: string;
  name: string;
  username?: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
  folder_id?: string;
  totp_secret?: string;
  created_at: number;
  updated_at: number;
}
