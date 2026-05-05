import { useEffect, useRef } from "react";

const ACTIVITY_EVENTS = ["mousemove", "mousedown", "keydown", "touchstart", "scroll"] as const;

/**
 * Locks the vault after `timeoutMs` milliseconds of inactivity.
 * Pass timeoutMs=0 to disable.
 * `active` lets the caller gate the hook (e.g. only when unlocked).
 */
export function useAutoLock(timeoutMs: number, onLock: () => void, active: boolean) {
  const onLockRef = useRef(onLock);
  onLockRef.current = onLock;

  useEffect(() => {
    if (!active || timeoutMs === 0) return;

    let timer: ReturnType<typeof setTimeout>;

    const reset = () => {
      clearTimeout(timer);
      timer = setTimeout(() => onLockRef.current(), timeoutMs);
    };

    reset();
    ACTIVITY_EVENTS.forEach((ev) => window.addEventListener(ev, reset, { passive: true }));

    return () => {
      clearTimeout(timer);
      ACTIVITY_EVENTS.forEach((ev) => window.removeEventListener(ev, reset));
    };
  }, [timeoutMs, active]);
}
