// Light/dark appearance (`theming` spec, design.md D6): follows the OS preference
// by default; an explicit toggle overrides it and persists in `localStorage` — the
// one piece of persisted user state in the product, kept in the browser (not a
// config file) to preserve the zero-config principle (PRD §4) and the promise that
// the CLI itself writes nothing to disk. No external fonts/stylesheets are used
// anywhere in the app, so both appearances render fully with the network disabled,
// consistent with the CSP and the file-mode privacy guarantee.

export type ThemeOverride = "light" | "dark";

const STORAGE_KEY = "looqlog-theme";

function readStoredOverride(): ThemeOverride | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === "light" || raw === "dark" ? raw : null;
  } catch {
    // Storage can throw (private-browsing quota, disabled storage) — fall back to
    // "no override", i.e. follow the system preference for this load only.
    return null;
  }
}

function applyOverride(override: ThemeOverride | null): void {
  const root = document.documentElement;
  if (override === null) {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", override);
  }
}

function effectiveTheme(override: ThemeOverride | null): ThemeOverride {
  if (override !== null) {
    return override;
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function labelFor(override: ThemeOverride | null): string {
  if (override === null) {
    return `Theme: Auto (${effectiveTheme(null)})`;
  }
  return `Theme: ${override === "dark" ? "Dark" : "Light"}`;
}

/** Wires up the `#theme-toggle` button (`web/index.html`) if present: applies the
 * stored override (or system preference) immediately, then cycles
 * Auto → Light → Dark → Auto on each click, persisting the explicit choices and
 * clearing back to "follow the system" on the third click. */
export function initTheme(): void {
  let override = readStoredOverride();
  applyOverride(override);

  const button = document.getElementById("theme-toggle") as HTMLButtonElement | null;
  if (!button) {
    return;
  }
  button.textContent = labelFor(override);
  button.addEventListener("click", () => {
    override = override === null ? "light" : override === "light" ? "dark" : null;
    applyOverride(override);
    try {
      if (override === null) {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, override);
      }
    } catch {
      // Non-persistable is not fatal — the choice still applies for this load.
    }
    button.textContent = labelFor(override);
  });
}
