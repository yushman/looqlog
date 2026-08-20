// Reads the per-process WebSocket auth token embedded in the served page
// (`security` spec, design.md D1). Presented as the first `/ws` message, never
// placed in the WebSocket URL/query string, so it never lands in shell history or a
// proxy access log.

/** Same `<template>` convention as `#hint-template`/`#mode-template`
 * (`components/looqlog-app.ts`): `.innerHTML` is the one that reflects a
 * `<template>`'s content, since its children live in an inert `.content`
 * DocumentFragment, not as direct children `.textContent` would see. Falls back to
 * the empty string both when the element is missing and when its content is still
 * the literal placeholder (`vite dev`, no backend to substitute it). */
export function readAuthToken(): string {
  const template = document.getElementById("token-template") as HTMLTemplateElement | null;
  const raw = template?.innerHTML.trim() ?? "";
  return raw === "__LOOQLOG_TOKEN__" ? "" : raw;
}
