import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Message from './Message.svelte';
import type { Agent, Message as MessageT } from '../types';

const herald: Agent = {
  id: 'herald',
  display: 'Herald',
  desc: 'gitconv',
  expectedHost: 'gitconv',
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  color: 'var(--ag-herald)',
  abbr: 'HE',
  wip: [],
};

const noteMessage: MessageT = {
  id: 'msg_01JQ001',
  from: 'herald',
  type: 'note',
  ts: '2026-07-17T14:02:00Z',
  host: 'gitconv',
  to: ['all'],
  cc: [],
  topic: 'reader',
  summary: 'Shipping confer 0.7.3',
  body: 'Shipping confer 0.7.3 — @all the `serve --all-hubs` broken-tab fix is in.',
  of: null,
  replyTo: null,
  supersedes: null,
  refs: [],
};

const claimMessage: MessageT = {
  ...noteMessage,
  id: 'msg_01JQa10',
  type: 'claim',
  summary: 'claimed req_01JQ8f2',
  body: 'Reader claimed this request.',
};

describe('Message', () => {
  it('renders a note with the who/role/ts head and the body text', () => {
    render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });

    expect(screen.getByText('Herald')).toBeInTheDocument();
    expect(screen.getByText('gitconv')).toBeInTheDocument();
    expect(screen.getByText(/Shipping confer 0.7.3/)).toBeInTheDocument();
  });

  it('highlights @mentions and inline code distinctly from the surrounding text', () => {
    const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });

    expect(container.querySelector('.mention')?.textContent).toBe('@all');
    expect(container.querySelector('code.mono')?.textContent).toBe('serve --all-hubs');
  });

  it('renders lifecycle types (claim/done/blocked) as an inline sysline, not a message bubble', () => {
    const { container } = render(Message, { message: claimMessage, fromAgent: herald, seenEntries: [] });

    expect(container.querySelector('.sysline')).toBeInTheDocument();
    expect(container.querySelector('.msg')).not.toBeInTheDocument();
    expect(screen.getByText('claimed req_01JQ8f2')).toBeInTheDocument();
  });

  it('shows the seen indicator reflecting all-seen vs partial state', () => {
    render(Message, {
      message: noteMessage,
      fromAgent: herald,
      seenEntries: [{ id: 'reader', name: 'Reader', color: 'var(--ag-reader)', ts: '14:03' }],
    });

    expect(screen.getByText('all seen')).toBeInTheDocument();
  });

  it('fires onSelect when the message body is clicked', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], onSelect });

    await user.click(screen.getByText('Herald'));

    expect(onSelect).toHaveBeenCalledWith('msg_01JQ001');
  });

  it('renders Markdown (headings/bold/lists) instead of literal source characters', () => {
    const mdMessage: MessageT = {
      ...noteMessage,
      id: 'msg_01JQ002',
      summary: 'Design review',
      body: '## Heading\n\n**bold** and a list:\n\n- one\n- two',
    };
    const { container } = render(Message, { message: mdMessage, fromAgent: herald, seenEntries: [] });

    expect(container.querySelector('h2')?.textContent).toBe('Heading');
    expect(container.querySelector('strong')?.textContent).toBe('bold');
    expect(container.querySelectorAll('li')).toHaveLength(2);
    expect(container.textContent).not.toContain('##');
    expect(container.textContent).not.toContain('**bold**');
  });

  describe('long-message clamping', () => {
    const longBody = Array.from({ length: 20 }, (_, i) => `Line ${i} of a long design-review post.`).join('\n');
    const longMessage: MessageT = { ...noteMessage, id: 'msg_01JQ003', summary: 'A long design review', body: longBody };

    it('clamps a long body and shows a Show more control, with the summary kept visible', () => {
      const { container } = render(Message, { message: longMessage, fromAgent: herald, seenEntries: [] });

      expect(screen.getByText('A long design review')).toBeInTheDocument();
      expect(container.querySelector('.text-wrap.clamped')).toBeInTheDocument();
      expect(screen.getByText(/Show more/)).toBeInTheDocument();
    });

    it('expands to the full rendered body on Show more, and re-collapses on Show less', async () => {
      const user = userEvent.setup();
      const { container } = render(Message, { message: longMessage, fromAgent: herald, seenEntries: [] });

      await user.click(screen.getByText(/Show more/));
      expect(container.querySelector('.text-wrap.clamped')).not.toBeInTheDocument();
      expect(screen.getByText(/Show less/)).toBeInTheDocument();
      // The full body is present, not just a truncated slice.
      expect(container.textContent).toContain('Line 19 of a long design-review post.');

      await user.click(screen.getByText(/Show less/));
      expect(container.querySelector('.text-wrap.clamped')).toBeInTheDocument();
    });

    it('does not clamp a short body', () => {
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });

      expect(container.querySelector('.text-wrap.clamped')).not.toBeInTheDocument();
      expect(screen.queryByText(/Show more/)).not.toBeInTheDocument();
    });
  });

  describe('summary-line wrapping', () => {
    it('renders the summary as a word-aware, 2-line-clamped lead (not a hard nowrap cutoff)', () => {
      const longSummary =
        'A genuinely long one-sentence summary that would previously be cut off mid-word by a hard single-line nowrap truncation';
      const message: MessageT = { ...noteMessage, id: 'msg_01JQ004', summary: longSummary };
      const { container } = render(Message, { message, fromAgent: herald, seenEntries: [], density: 'summary' });

      const lead = container.querySelector('.summary-line .lead');
      expect(lead).toBeInTheDocument();
      expect(lead?.textContent).toBe(longSummary);
      // Word-aware wrapping, not the old `white-space: nowrap` single-line cut.
      const style = lead ? getComputedStyle(lead) : null;
      expect(style?.whiteSpace).not.toBe('nowrap');
    });
  });

  describe('summary density', () => {
    it('shows only the summary line and hides the body until expanded', () => {
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'summary' });

      expect(screen.getByText('Shipping confer 0.7.3')).toBeInTheDocument();
      expect(container.querySelector('.text-wrap')).not.toBeInTheDocument();
      expect(container.textContent).not.toContain('serve --all-hubs');
    });

    it('reveals the full rendered body on expand, and hides it again on collapse', async () => {
      const user = userEvent.setup();
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'summary' });

      const chevron = container.querySelector('.expand-toggle') as HTMLButtonElement;
      expect(chevron).toBeInTheDocument();
      // A readable label, not a bare unlabeled glyph.
      expect(chevron.textContent).toContain('Show more');

      await user.click(chevron);
      expect(container.querySelector('.text-wrap')).toBeInTheDocument();
      expect(container.querySelector('.mention')?.textContent).toBe('@all');
      expect(chevron.textContent).toContain('Show less');

      await user.click(chevron);
      expect(container.querySelector('.text-wrap')).not.toBeInTheDocument();
      expect(chevron.textContent).toContain('Show more');
    });

    it('clicking anywhere on the summary line (not just the chevron) also expands the body', async () => {
      const user = userEvent.setup();
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'summary' });

      await user.click(screen.getByText('Shipping confer 0.7.3'));
      expect(container.querySelector('.text-wrap')).toBeInTheDocument();
    });

    it('clicking the chevron toggles exactly once, not twice (no double-toggle from event bubbling to the summary line)', async () => {
      const user = userEvent.setup();
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'summary' });

      const chevron = container.querySelector('.expand-toggle') as HTMLButtonElement;
      await user.click(chevron);

      // A double-toggle (chevron handler + bubbled summary-line handler both
      // firing) would net out to "still collapsed" — assert it actually
      // expanded.
      expect(container.querySelector('.text-wrap')).toBeInTheDocument();
    });

    it('expanding one message does not affect another (independent per-message state)', async () => {
      const user = userEvent.setup();
      const other: MessageT = { ...noteMessage, id: 'msg_01JQ009', summary: 'Another note', body: 'A separate body.' };

      const { container: c1 } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'summary' });
      const { container: c2 } = render(Message, { message: other, fromAgent: herald, seenEntries: [], density: 'summary' });

      await user.click(c1.querySelector('.expand-toggle') as HTMLButtonElement);

      expect(c1.querySelector('.text-wrap')).toBeInTheDocument();
      expect(c2.querySelector('.text-wrap')).not.toBeInTheDocument();
    });

    it('in full density, no chevron is shown and the body is always visible', () => {
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], density: 'full' });

      expect(container.querySelector('.expand-toggle')).not.toBeInTheDocument();
      expect(container.querySelector('.text-wrap')).toBeInTheDocument();
    });
  });

  describe('copy-id affordance (design/41 Phase 0)', () => {
    afterEach(() => {
      delete (navigator as { clipboard?: unknown }).clipboard;
    });

    it('exposes a copy-id control for the bare message id, which copies without also selecting the message, and flips to the copied state', async () => {
      // userEvent.setup() installs its own navigator.clipboard stub, so our
      // mock must be defined AFTER setup() or it gets clobbered.
      const user = userEvent.setup();
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
      const onSelect = vi.fn();

      render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], onSelect });

      const copyBtn = screen.getByRole('button', { name: /copy id msg_01JQ001/i });
      await user.click(copyBtn);

      await vi.waitFor(() => {
        expect(writeText).toHaveBeenCalledWith('msg_01JQ001');
        // Same observable success feedback MetaThread's `.gid` copy uses:
        // aria-label swaps to "Copied …" and the `copied` class is set.
        expect(copyBtn).toHaveAttribute('aria-label', 'Copied msg_01JQ001');
      });
      expect(copyBtn.className).toMatch(/copied/);
      // Clicking the nested copy button must never ALSO fire the row's own
      // onclick (selectMessage) — this is the specific mechanism the bug
      // report suspected (nested button inside a role="button" row).
      expect(onSelect).not.toHaveBeenCalled();
    });
  });

  describe('keyboard-architecture pass: mouse path for the `f` shortcut', () => {
    it('hides the "open in focus reader" button when onOpenFocus is not wired (no dead affordance)', () => {
      render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });
      expect(screen.queryByRole('button', { name: 'Open in focus reader' })).not.toBeInTheDocument();
    });

    it('fires onOpenFocus (not onSelect) when clicked, without also selecting via the row click handler', async () => {
      const user = userEvent.setup();
      const onOpenFocus = vi.fn();
      const onSelect = vi.fn();
      render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], onOpenFocus, onSelect });

      await user.click(screen.getByRole('button', { name: 'Open in focus reader' }));

      expect(onOpenFocus).toHaveBeenCalledWith('msg_01JQ001');
      expect(onSelect).not.toHaveBeenCalled();
    });
  });

  describe('scroll-to highlight pulse', () => {
    it('applies the pulse class when highlight is true, and not otherwise', () => {
      const { container: withHighlight } = render(Message, {
        message: noteMessage,
        fromAgent: herald,
        seenEntries: [],
        highlight: true,
      });
      expect(withHighlight.querySelector('.msg.pulse')).toBeInTheDocument();

      const { container: without } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });
      expect(without.querySelector('.msg.pulse')).not.toBeInTheDocument();
    });

    it('tags the root element with data-msg-id so a scroll target can be found via querySelector', () => {
      const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });
      expect(container.querySelector('[data-msg-id="msg_01JQ001"]')).toBeInTheDocument();
    });

    it('applies the pulse class to a sysline root too (claim/done/etc. can be scroll targets)', () => {
      const { container } = render(Message, { message: claimMessage, fromAgent: herald, seenEntries: [], highlight: true });
      expect(container.querySelector('.sysline.pulse')).toBeInTheDocument();
      expect(container.querySelector('[data-msg-id="msg_01JQa10"]')).toBeInTheDocument();
    });
  });
});
