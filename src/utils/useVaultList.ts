import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { listVaults, VaultList } from "../api";

/** The vaults on this device, kept current as they are added, renamed,
 *  removed or switched between. Null until first loaded. */
export function useVaultList(): VaultList | null {
  const [list, setList] = useState<VaultList | null>(null);

  useEffect(() => {
    const load = () => listVaults().then(setList).catch(() => {});
    load();
    const unlisten = listen("vault-list-changed", load);
    return () => { unlisten.then((f) => f()); };
  }, []);

  return list;
}
