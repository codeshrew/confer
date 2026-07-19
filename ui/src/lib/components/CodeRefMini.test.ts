import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { api } from '../api';
import CodeRefMini from './CodeRefMini.svelte';
import type { CodeRef } from '../types';

vi.mock('../api', () => ({
  api: { getRefs: vi.fn().mockResolvedValue([]) },
}));

const ref: CodeRef = {
  repo: 'confer',
  path: 'src/patch.rs',
  sha: 'abc123',
  range: [47, 73],
  contentHash: null,
  refName: null,
  refType: null,
  commitDate: null,
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

describe('CodeRefMini', () => {
  it('renders the path, line range, and repo — no snippet fetch on mount', () => {
    render(CodeRefMini, { ref, hub: 'confer-lab' });
    expect(screen.getByText('src/patch.rs')).toBeInTheDocument();
    expect(screen.getByText('L47–73')).toBeInTheDocument();
    expect(screen.getByText('confer')).toBeInTheDocument();
    expect(api.getRefs).not.toHaveBeenCalled();
  });

  it('clicking fetches real reverse-index hits and calls onOpenRefs', async () => {
    const user = userEvent.setup();
    const onOpenRefs = vi.fn();
    vi.mocked(api.getRefs).mockResolvedValue([]);
    render(CodeRefMini, { ref, hub: 'confer-lab', onOpenRefs });

    await user.click(screen.getByTestId('code-ref-mini'));

    expect(api.getRefs).toHaveBeenCalledWith('confer-lab', 'confer:src/patch.rs@47-73', true);
    expect(onOpenRefs).toHaveBeenCalledWith(ref, []);
  });

  it('gets a `.sel` ring when selected', () => {
    const { container } = render(CodeRefMini, { ref, hub: 'confer-lab', selected: true });
    expect(container.querySelector('.c-mini.sel')).toBeInTheDocument();
  });
});
