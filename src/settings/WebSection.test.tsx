import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/preact';
import { WebSection } from './WebSection';

afterEach(cleanup);

describe('联网搜索开关', () => {
  it('点一下保存；保存失败时开关弹回去', async () => {
    const webSave = vi.fn(async (enabled: boolean) => ({ enabled }));
    render(<WebSection api={{ webSave }} initial={true} />);
    const box = screen.getByRole('checkbox') as HTMLInputElement;
    expect(box.checked).toBe(true);
    fireEvent.click(box);
    expect(webSave).toHaveBeenCalledWith(false);
    await waitFor(() => expect(box.checked).toBe(false));
    cleanup();
    const bad = vi.fn(async () => {
      throw new Error('x');
    });
    render(<WebSection api={{ webSave: bad }} initial={true} />);
    const box2 = screen.getByRole('checkbox') as HTMLInputElement;
    fireEvent.click(box2);
    await waitFor(() => expect(box2.checked).toBe(true));
  });

  it('写明会发给谁、发什么', () => {
    render(<WebSection api={{ webSave: async (enabled: boolean) => ({ enabled }) }} initial={true} />);
    expect(screen.getByText(/DuckDuckGo/)).toBeTruthy();
    expect(screen.getByText(/只会把搜索关键词/)).toBeTruthy();
  });
});
