import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { VisionSection } from './VisionSection';
import type { VisionSettings } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('看一眼屏幕设置', () => {
  it('没有能看图的模型时不能打开；打开后能选主动看的场合、排除游戏、看记录', async () => {
    const saved: VisionSettings[] = [];
    const api = {
      saveVision: async (v: VisionSettings) => {
        saved.push(v);
        return v;
      },
      visionCost: async (): Promise<[number, number]> => [3, 0.012],
      visionPreview: async () => ({ image: 'data:image/jpeg;base64,QUJD', app: 'Hades', masked: 2, ocr_missing: false, refused: null }),
      visionRecords: async () => [{ at: 1_700_000_000, trigger: 'auto' as const, app: 'Hades', outcome: 'sent', masked: 1 }],
    };
    const view = testView();
    render(<VisionSection api={api} view={view} games={['Hades']} />);
    await flush();
    const box = screen.getByText('允许她看屏幕').closest('label')!.querySelector('input')!;
    expect(box.disabled).toBe(true);
    expect(screen.getByText(/累计看了 3 次/)).toBeTruthy();
    cleanup();
    view.backends.push({ ...view.backends[0], config: { ...view.backends[0].config, id: 'claude', kind: 'anthropic', name: 'Claude' } });
    render(<VisionSection api={api} view={view} games={['Hades']} />);
    await flush();
    fireEvent.click(screen.getByText('允许她看屏幕').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)).toMatchObject({ enabled: true, backend: 'claude' });
    expect(screen.getByText(/涂黑 1 处/).textContent).toContain('她主动看');
    fireEvent.click(screen.getByText('看视频时').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.auto_categories).toEqual(['game', 'video']);
    fireEvent.click(screen.getByText('Hades').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.exclude).toEqual(['Hades']);
  });

  it('预览：倒计时后只在设置里显示涂黑后的图', async () => {
    vi.useFakeTimers();
    const view = testView();
    view.vision = { ...view.vision, enabled: true, backend: 'claude' };
    view.backends.push({ ...view.backends[0], config: { ...view.backends[0].config, id: 'claude', kind: 'anthropic', name: 'Claude' } });
    let previews = 0;
    const api = {
      saveVision: async (v: VisionSettings) => v,
      visionCost: async (): Promise<[number, number]> => [0, 0],
      visionPreview: async () => {
        previews++;
        return { image: 'data:image/jpeg;base64,QUJD', app: 'Hades', masked: 2, ocr_missing: false, refused: null };
      },
      visionRecords: async () => [],
    };
    render(<VisionSection api={api} view={view} games={[]} />);
    fireEvent.click(screen.getByText('预览一次'));
    expect(previews).toBe(0);
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    vi.useRealTimers();
    await flush();
    expect(previews).toBe(1);
    expect(screen.getByText(/涂黑了 2 处/)).toBeTruthy();
    expect(screen.getByAltText('她会看到的画面')).toBeTruthy();
  });
});
