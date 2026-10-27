import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { ModelsPage } from './ModelsPage';
import type { SettingsApi } from '../lib/ipc';
import type { SettingsView } from '../lib/types';

afterEach(cleanup);

const view: SettingsView = {
  general: { always_on_top: true, pet_scale: 1, idle_animation: true, dnd: false },
  backends: [
    {
      config: {
        id: 'deepseek',
        kind: 'deepseek',
        name: 'DeepSeek',
        base_url: 'https://api.deepseek.com',
        model: 'deepseek-flash',
        reasoning: 'disabled',
        price: { input: 0.3, cached_input: 0.006, cache_write: 0.3, output: 1.2 },
      },
      kind_name: 'DeepSeek',
      needs_key: true,
      key_url: 'https://platform.deepseek.com/api_keys',
      note: '',
      key_mask: null,
      removable: false,
    },
  ],
  kinds: [{ kind: 'deepseek', display_name: 'DeepSeek' }],
  chat_backend: 'deepseek',
  background_backend: '',
  budget: { daily_usd: 1, monthly_usd: 10 },
  data_dir: 'C:\\data',
  version: '0.1.0',
  initial_tab: null,
};

function fakeApi() {
  return {
    setKey: vi.fn(async () => '••••7890'),
    test: vi.fn(async () => ({ ok: true, latency_ms: 120, reply: '你好', error_kind: '', error_detail: '', hint: '' })),
    select: vi.fn(async () => {}),
  } as unknown as SettingsApi & { setKey: ReturnType<typeof vi.fn> };
}

describe('ModelsPage', () => {
  it('保存 Key 后立即清空输入框，Key 不会留在界面上', async () => {
    const api = fakeApi();
    const onChanged = vi.fn();
    render(<ModelsPage api={api} view={view} onChanged={onChanged} />);
    const input = screen.getByLabelText('DeepSeek 的 API Key') as HTMLInputElement;
    expect(input.type).toBe('password');
    fireEvent.input(input, { target: { value: ' secret-key-1234567890 ' } });
    await act(async () => {
      fireEvent.click(screen.getByText('保存 Key'));
    });
    expect(api.setKey).toHaveBeenCalledWith('deepseek', ' secret-key-1234567890 ');
    expect(input.value).toBe('');
    expect(document.body.textContent).not.toContain('secret-key-1234567890');
    expect(onChanged).toHaveBeenCalled();
  });

  it('没有 Key 时不能测试连接', () => {
    render(<ModelsPage api={fakeApi()} view={view} onChanged={() => {}} />);
    expect((screen.getByText('测试连接') as HTMLButtonElement).disabled).toBe(true);
  });

  it('有 Key 时只显示末 4 位', () => {
    const v = { ...view, backends: [{ ...view.backends[0], key_mask: '••••abcd' }] };
    render(<ModelsPage api={fakeApi()} view={v} onChanged={() => {}} />);
    expect(screen.getByText('已保存 ••••abcd')).toBeTruthy();
  });
});
