<script lang="ts">
  // File-type icon for a code-file row — extension → curated Material-subset
  // icon (icons.ts's `ft` table), with a short filename-override map for
  // names whose type isn't decidable from the extension alone (or that have
  // none), and a Lucide `file` fallback (tinted `--muted`) for anything
  // unmapped. design/43 Thread 3: never a whole-set import, hand-picked only.
  import Icon from './Icon.svelte';

  interface Props {
    /** Full path or bare filename — only the basename/extension matter. */
    path: string;
    size?: number;
  }

  let { path, size = 15 }: Props = $props();

  const EXT_MAP: Record<string, string> = {
    rs: 'rust',
    ts: 'typescript',
    mts: 'typescript',
    cts: 'typescript',
    tsx: 'react',
    jsx: 'react',
    js: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    svelte: 'svelte',
    py: 'python',
    swift: 'swift',
    go: 'go',
    md: 'markdown',
    mdx: 'markdown',
    json: 'json',
    toml: 'toml',
    yaml: 'yaml',
    yml: 'yaml',
    css: 'css',
    scss: 'css',
    html: 'html',
    htm: 'html',
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    sql: 'sql',
    rb: 'ruby',
    c: 'c',
    h: 'c',
    cpp: 'cpp',
    cc: 'cpp',
    cxx: 'cpp',
    hpp: 'cpp',
    java: 'java',
    png: 'image',
    jpg: 'image',
    jpeg: 'image',
    gif: 'image',
    svg: 'image',
    webp: 'image',
    lock: 'lock',
  };

  // Filenames the extension map gets wrong or that have no extension at all.
  const NAME_MAP: Record<string, string> = {
    'package.json': 'json',
    'package-lock.json': 'lock',
    'cargo.lock': 'lock',
    'pnpm-lock.yaml': 'lock',
    dockerfile: 'settings',
    makefile: 'settings',
    '.gitignore': 'settings',
  };

  function basename(p: string): string {
    return p.split('/').pop() || p;
  }

  const iconName = $derived.by((): string | null => {
    const name = basename(path).toLowerCase();
    if (NAME_MAP[name]) return NAME_MAP[name];
    const dot = name.lastIndexOf('.');
    const ext = dot > 0 ? name.slice(dot + 1) : '';
    return (ext && EXT_MAP[ext]) || null;
  });
</script>

{#if iconName}
  <Icon name={iconName} {size} />
{:else}
  <Icon name="file" {size} class="unmapped-icon" />
{/if}

<style>
  :global(.unmapped-icon) {
    color: var(--muted);
  }
</style>
