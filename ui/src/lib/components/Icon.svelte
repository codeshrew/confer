<script lang="ts">
  // Generic renderer for icons.ts's inline SVG strings — looks a name up in
  // either the Lucide (`ui`, monochrome/stroke) or Material-subset (`ft`,
  // full-color/fill) tables and renders it via `{@html}`. Safe: both tables
  // are hand-picked, hardcoded, trusted local content — never peer input —
  // so no sanitize pass is needed (unlike message bodies in markdown.ts).
  import { uiIcons, ftIcons } from '../icons';

  interface Props {
    /** Looked up in `uiIcons` first, then `ftIcons` — callers don't need to
     * know which table an icon lives in. */
    name: string;
    size?: number;
    class?: string;
  }

  let { name, size = 15, class: klass = '' }: Props = $props();

  const isUi = $derived(name in uiIcons);
  const def = $derived(uiIcons[name] ?? ftIcons[name]);
</script>

{#if def}
  <svg
    class="icon {klass}"
    class:stroke={isUi}
    viewBox={def.viewBox}
    width={size}
    height={size}
    aria-hidden="true"
    focusable="false"
    >{@html def.markup}</svg
  >
{/if}

<style>
  .icon {
    flex: 0 0 auto;
    display: block;
  }
  /* Lucide icons ship as stroke-only path data (no fill/stroke attrs of
     their own — those lived on the outer <svg> we stripped when building
     icons.ts) — supply them here so they inherit `currentColor` from
     whatever text color the caller sets, same as every other UI glyph. */
  .icon.stroke {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
