import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { VoicePage } from './VoicePage';
import type { VoiceSettings } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);
const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('声音设置', () => {
  it('默认关闭；选了 Fish Audio 才能打开，Key 只显示末尾 4 位', async () => {
    const saved: VoiceSettings[] = [];
    const calls: string[] = [];
    const api = {
      voiceSave: async (v: VoiceSettings) => {
        saved.push(v);
        return { ...v, enabled: v.enabled && !!v.engine };
      },
      voiceSetKey: async (w: 'fish' | 'openai', k: string) => {
        calls.push(`key:${w}:${k}`);
        return k.slice(-4);
      },
      voiceClearKey: async () => {},
      voiceTest: async () => {
        calls.push('test');
      },
      voiceClearCache: async () => 0,
    };
    render(<VoicePage api={api} view={testView()} />);
    const toggle = screen.getByText('让她出声').closest('label')!.querySelector('input')!;
    expect(toggle.disabled).toBe(true);
    fireEvent.click(screen.getByText(/Fish Audio（推荐/).closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.engine).toBe('fish');
    expect(screen.getByText('还没填')).toBeTruthy();
    fireEvent.input(screen.getByLabelText('语音服务 Key'), { target: { value: 'fish-key-1234' } });
    fireEvent.click(screen.getByText('保存 Key'));
    await flush();
    expect(calls).toContain('key:fish:fish-key-1234');
    expect(screen.getByText('已保存（…1234）')).toBeTruthy();
    fireEvent.click(screen.getByText('试听一句'));
    await flush();
    expect(calls).toContain('test');
    fireEvent.click(screen.getByText('让她出声').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.enabled).toBe(true);
  });
});
