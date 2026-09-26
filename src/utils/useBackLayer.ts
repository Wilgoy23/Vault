import { useEffect, useRef } from "react";

/** Layers currently claiming the back button, innermost last. */
interface Layer {
  close: () => void;
  /** Set when a back press consumed this layer's history entry. */
  popped: boolean;
}

const layers: Layer[] = [];
let listening = false;
/** Pops this module asked for itself, which must not close another layer. */
let selfPops = 0;

function onPopState() {
  if (selfPops > 0) {
    selfPops--;
    return;
  }
  const top = layers.pop();
  if (!top) return;
  top.popped = true;
  top.close();
}

/**
 * Makes Android's system back button dismiss one layer of UI instead of
 * leaving the app.
 *
 * Every open layer pushes a history entry, so a back press pops that entry
 * and closes the innermost layer. Closing a layer any other way — a Cancel
 * button, Escape, tapping the back arrow — removes the entry again, so the
 * history stack stays in step with what is on screen.
 *
 * Pass `active: false` on desktop; nothing should touch history there.
 */
export function useBackLayer(active: boolean, close: () => void) {
  const latest = useRef(close);
  latest.current = close;

  useEffect(() => {
    if (!active) return;

    const layer: Layer = { close: () => latest.current(), popped: false };
    layers.push(layer);
    history.pushState({ vaultBackLayer: true }, "");

    // The listener stays for the life of the page: a pending self-pop has to
    // reach it, and with no layers registered it does nothing anyway.
    if (!listening) {
      window.addEventListener("popstate", onPopState);
      listening = true;
    }

    return () => {
      const i = layers.indexOf(layer);
      if (i !== -1) layers.splice(i, 1);
      // Closed from inside the app, so the entry this layer pushed is still
      // on the stack — drop it, or the next back press would be swallowed.
      if (!layer.popped) {
        selfPops++;
        history.back();
      }
    };
  }, [active]);
}
