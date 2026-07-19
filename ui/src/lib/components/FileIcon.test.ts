import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import FileIcon from './FileIcon.svelte';

describe('FileIcon', () => {
  it('maps common extensions to their file-type icon', () => {
    const { container } = render(FileIcon, { path: 'src/main.rs' });
    // rust.svg's fill color is the cheapest signal that the RIGHT icon
    // rendered (each ft icon has a distinct brand-ish fill).
    expect(container.querySelector('svg')?.innerHTML).toContain('#ff7043');
  });

  it('resolves the extension off the basename, ignoring directory components', () => {
    const { container } = render(FileIcon, { path: 'a/b/c/index.ts' });
    expect(container.querySelector('svg')?.innerHTML).toContain('#0288d1'); // typescript
  });

  it('applies filename overrides ahead of the extension map', () => {
    const { container } = render(FileIcon, { path: 'package.json' });
    expect(container.querySelector('svg')?.innerHTML).toContain('#f9a825'); // json
  });

  it('overrides extensionless config filenames (Dockerfile, Makefile)', () => {
    const dockerfile = render(FileIcon, { path: 'Dockerfile' });
    const makefile = render(FileIcon, { path: 'Makefile' });
    // Both map to the gear/settings icon — its path data includes this
    // distinctive full-square background rect unique to that icon.
    expect(dockerfile.container.querySelector('svg')?.innerHTML).toContain('M0 0h24v24H0z');
    expect(makefile.container.querySelector('svg')?.innerHTML).toContain('M0 0h24v24H0z');
  });

  it('falls back to the Lucide file glyph, tinted muted, for unmapped extensions', () => {
    const { container } = render(FileIcon, { path: 'notes.xyz' });
    const svg = container.querySelector('svg');
    expect(svg).toHaveClass('unmapped-icon');
    expect(svg?.classList.contains('stroke')).toBe(true);
  });

  it('falls back for extensionless, non-overridden filenames', () => {
    const { container } = render(FileIcon, { path: 'LICENSE' });
    expect(container.querySelector('svg')).toHaveClass('unmapped-icon');
  });
});
