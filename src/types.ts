export interface Entry {
  id: string;
  name: string;
  email: string;
  password: string;
  url?: string;
  notes?: string;
  created_at: number;
  updated_at: number;
}