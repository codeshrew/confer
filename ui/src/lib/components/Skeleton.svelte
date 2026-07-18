<script lang="ts">
  // Ports `.skeleton`/`.sk-*` from design/serve-dashboard-v2-mockup.html — a
  // shimmering loading placeholder, respecting prefers-reduced-motion (the
  // shimmer keyframe is disabled via media query in the stylesheet below).
  interface Props {
    rows?: number;
  }

  let { rows = 3 }: Props = $props();
  const rowIndexes = $derived(Array.from({ length: rows }, (_, i) => i));
</script>

<div class="skeleton" data-testid="skeleton">
  {#each rowIndexes as i (i)}
    <div class="sk-row">
      <span class="sk sk-av"></span>
      <div class="sk-lines">
        <span class="sk sk-l1" style={i % 2 ? 'width:70%' : undefined}></span>
        <span class="sk sk-l2" style={i % 3 === 0 ? 'width:80%' : undefined}></span>
      </div>
    </div>
  {/each}
</div>

<style>
  .skeleton {
    padding: 0 20px;
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .sk-row {
    display: flex;
    gap: 12px;
  }
  .sk {
    display: block;
    border-radius: 6px;
    background: linear-gradient(100deg, var(--panel-2) 30%, var(--panel-3) 50%, var(--panel-2) 70%);
    background-size: 200% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }
  @keyframes shimmer {
    0% {
      background-position: 120% 0;
    }
    100% {
      background-position: -20% 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sk {
      animation: none;
    }
  }
  .sk-av {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    flex: 0 0 auto;
  }
  .sk-lines {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding-top: 2px;
  }
  .sk-l1 {
    height: 11px;
    width: 45%;
  }
  .sk-l2 {
    height: 11px;
    width: 85%;
  }
</style>
