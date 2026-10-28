import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { ModelsPage } from './ModelsPage';
import type { SettingsApi } from '../lib/ipc';
import type { SettingsView } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);

const view: SettingsView = testView();

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
