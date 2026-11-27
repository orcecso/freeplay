import { useEffect, useRef, useState } from "react";

interface GamepadNavOptions {
  onPrevTab: () => void;
  onNextTab: () => void;
  onBack: () => void;
}

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function getFocusableElements(): HTMLElement[] {
  const modal = document.querySelector<HTMLElement>(".modal-backdrop");
  const root: ParentNode = modal ?? document.querySelector(".main") ?? document;
  const list = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
  return list.filter((el) => {
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && getComputedStyle(el).visibility !== "hidden";
  });
}

function moveFocus(delta: number) {
  const items = getFocusableElements();
  if (items.length === 0) return;

  const active = document.activeElement as HTMLElement | null;
  const currentIdx = active ? items.indexOf(active) : -1;
  let nextIdx: number;
  if (currentIdx === -1) {
    nextIdx = delta > 0 ? 0 : items.length - 1;
  } else {
    nextIdx = (currentIdx + delta + items.length) % items.length;
  }

  const target = items[nextIdx];
  target.focus({ preventScroll: false });
  target.scrollIntoView({ block: "nearest", behavior: "smooth" });
}

/**
 * Lightweight HTML5 Gamepad API hook for Steam Deck, ROG Ally, Legion Go, and
 * standard XInput/DualSense controllers:
 * - LB / RB: Switch sidebar tabs
 * - D-Pad / Left Stick: Move focus across buttons and inputs
 * - A: Click / activate focused control
 * - B: Back / close modal
 * - Right Stick Y: Smooth scroll active pane
 */
export function useGamepadNav({ onPrevTab, onNextTab, onBack }: GamepadNavOptions): boolean {
  const [connected, setConnected] = useState(false);
  const callbacksRef = useRef({ onPrevTab, onNextTab, onBack });
  callbacksRef.current = { onPrevTab, onNextTab, onBack };

  useEffect(() => {
    if (typeof navigator === "undefined" || typeof navigator.getGamepads !== "function") {
      return;
    }

    let rafId: number | null = null;
    const prevButtons = new Map<number, boolean>();
    let lastStickMove = 0;

    const hasActiveGamepad = () => {
      const pads = navigator.getGamepads();
      return Array.from(pads).some((p) => p && p.connected);
    };

    const poll = (now: number) => {
      const pads = navigator.getGamepads();
      const pad = Array.from(pads).find((p) => p && p.connected);
      if (!pad) {
        setConnected(false);
        document.documentElement.removeAttribute("data-gamepad");
        rafId = null;
        return;
      }

      const pressed = (idx: number) => Boolean(pad.buttons[idx]?.pressed);
      const justPressed = (idx: number) => {
        const cur = pressed(idx);
        const prev = prevButtons.get(idx) ?? false;
        prevButtons.set(idx, cur);
        return cur && !prev;
      };

      let interacted = false;

      if (justPressed(4)) {
        // LB
        callbacksRef.current.onPrevTab();
        interacted = true;
      }
      if (justPressed(5)) {
        // RB
        callbacksRef.current.onNextTab();
        interacted = true;
      }
      if (justPressed(12) || justPressed(14)) {
        // D-Pad Up / Left
        moveFocus(-1);
        interacted = true;
      }
      if (justPressed(13) || justPressed(15)) {
        // D-Pad Down / Right
        moveFocus(1);
        interacted = true;
      }
      if (justPressed(0)) {
        // A button
        const active = document.activeElement as HTMLElement | null;
        if (active && active !== document.body) {
          active.click();
        } else {
          moveFocus(1);
        }
        interacted = true;
      }
      if (justPressed(1)) {
        // B button
        callbacksRef.current.onBack();
        interacted = true;
      }

      // Left stick navigation with repeat debounce
      const leftY = pad.axes[1] ?? 0;
      if (Math.abs(leftY) > 0.65 && now - lastStickMove > 190) {
        moveFocus(leftY > 0 ? 1 : -1);
        lastStickMove = now;
        interacted = true;
      }

      // Right stick smooth scrolling
      const rightY = pad.axes[3] ?? 0;
      if (Math.abs(rightY) > 0.25) {
        const scroller =
          document.querySelector<HTMLElement>(".modal-backdrop .process-list") ??
          document.querySelector<HTMLElement>(".main");
        scroller?.scrollBy({ top: rightY * 14 });
      }

      if (interacted) {
        document.documentElement.setAttribute("data-gamepad", "true");
      }

      rafId = requestAnimationFrame(poll);
    };

    const startIfNeeded = () => {
      if (hasActiveGamepad()) {
        setConnected(true);
        if (rafId === null) {
          rafId = requestAnimationFrame(poll);
        }
      } else {
        setConnected(false);
        document.documentElement.removeAttribute("data-gamepad");
      }
    };

    window.addEventListener("gamepadconnected", startIfNeeded);
    window.addEventListener("gamepaddisconnected", startIfNeeded);
    startIfNeeded();

    return () => {
      window.removeEventListener("gamepadconnected", startIfNeeded);
      window.removeEventListener("gamepaddisconnected", startIfNeeded);
      if (rafId !== null) cancelAnimationFrame(rafId);
      document.documentElement.removeAttribute("data-gamepad");
    };
  }, []);

  return connected;
}
