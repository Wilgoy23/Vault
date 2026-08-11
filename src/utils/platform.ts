import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

let cached: Promise<string> | null = null;

/** Resolves to "windows" | "macos" | "linux" | "ios" | "android". */
export const getPlatform = (): Promise<string> => {
  if (!cached) cached = invoke<string>("get_platform").catch(() => "windows");
  return cached;
};

export const isMobilePlatform = async (): Promise<boolean> => {
  const p = await getPlatform();
  return p === "ios" || p === "android";
};

/** True on iOS/Android. Defaults to false until the platform is known. */
export function useIsMobile(): boolean {
  const [mobile, setMobile] = useState(false);
  useEffect(() => {
    isMobilePlatform().then(setMobile);
  }, []);
  return mobile;
}
