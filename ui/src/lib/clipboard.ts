// Copy-to-clipboard helper shared by every copy-id affordance (Message,
// BoardRow, RequestDetail's ctx-head, MetaThread's `.gid` line).
//
// REAL GOTCHA (design/41 §4): `navigator.clipboard` requires a secure
// context — https or localhost. `serve.rs` explicitly advertises the LAN IP
// for phone access (`http://10.0.0.x:8422`), which is plain HTTP and does
// NOT qualify. Without a fallback, copy silently no-ops exactly where
// Stefan uses his phone. So: try the modern async Clipboard API first, and
// if it's unavailable OR throws, fall back to the classic
// `document.execCommand('copy')` hidden-textarea trick, which works over
// plain HTTP.
export async function copyToClipboard(text: string): Promise<boolean> {
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Fall through to the legacy path below — a thrown/rejected
      // writeText (permission denied, non-secure context in some browsers)
      // must not be treated as "copy failed for good", just "try the
      // fallback".
    }
  }
  return legacyCopy(text);
}

/** The `execCommand('copy')` fallback: a hidden, off-screen, but focusable
 * textarea gets the text selected and copied via the browser's own copy
 * command, which — unlike the Clipboard API — has no secure-context
 * requirement. Returns false (rather than throwing) if the browser has
 * neither mechanism (e.g. this runs outside a DOM at all). */
function legacyCopy(text: string): boolean {
  if (typeof document === 'undefined') return false;
  const textarea = document.createElement('textarea');
  textarea.value = text;
  // Keep it out of the visible viewport and out of the tab order, but NOT
  // display:none/visibility:hidden — some browsers refuse to focus/select
  // text in an element that isn't actually rendered.
  textarea.setAttribute('readonly', '');
  textarea.style.position = 'fixed';
  textarea.style.top = '0';
  textarea.style.left = '-9999px';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  const previousFocus = document.activeElement as HTMLElement | null;
  try {
    textarea.select();
    textarea.setSelectionRange(0, text.length);
    return document.execCommand('copy');
  } catch {
    return false;
  } finally {
    document.body.removeChild(textarea);
    previousFocus?.focus?.();
  }
}
