import { afterEach, describe, expect, it } from 'vitest';
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
  it('没有能看图的模型时不能打开；有的话打开后能选游戏', async () => {
    const saved: VisionSettings[] = [];
    const api = {
      saveVision: async (v: VisionSettings) => {
        saved.push(v);
        return v;
      },
      visionCost: async (): Promise<[number, number]> => [3, 0.012],
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
    fireEvent.click(screen.getByText('Hades').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.games).toEqual(['Hades']);
  });
});
