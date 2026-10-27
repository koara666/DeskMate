import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { TaskCard } from './TaskCard';
import type { TaskApi } from '../lib/ipc';
import type { TaskEvent } from '../lib/types';

afterEach(cleanup);

function fakeApi(codexFound = true) {
  let handler: ((e: TaskEvent) => void) | null = null;
  const api = {
    defaults: vi.fn(async () => ({
      settings: {
        executor: 'claude' as const,
        root: 'D:\\DeskMate-Work',
        timeout_minutes: 10,
        budget_usd: 0.5,
        claude_model: '',
        codex_model: '',
        claude_path: '',
        codex_path: '',
      },
      root: 'D:\\DeskMate-Work',
      claude: { found: true, path: 'claude.exe', version: '2.1' },
      codex: { found: codexFound, path: '', version: '' },
    })),
    start: vi.fn(async (_g: string, _w: string, _e: string, _a: boolean, onEvent: (e: TaskEvent) => void) => {
      handler = onEvent;
      return 't1';
    }),
    approve: vi.fn(async (_id: string, _idx: number[], onEvent: (e: TaskEvent) => void) => {
      handler = onEvent;
    }),
    decline: vi.fn(async () => {}),
    cancel: vi.fn(async () => true),
    report: vi.fn(async () => {}),
  };
  const emit = (e: TaskEvent) => act(() => handler!(e));
  return { api: api as unknown as TaskApi & typeof api, emit };
}

async function mount(api: TaskApi, onFinished = vi.fn()) {
  render(<TaskCard api={api} initialGoal="整理截图" onClose={() => {}} onFinished={onFinished} />);
  await act(async () => {});
  return onFinished;
}

describe('TaskCard 任务卡片', () => {
  it('只有点「开始执行」才会启动；完成后请角色转述结果', async () => {
    const { api, emit } = fakeApi();
    const onFinished = await mount(api);
    expect(api.start).not.toHaveBeenCalled();
    expect((screen.getByLabelText('工作目录') as HTMLInputElement).value).toBe('D:\\DeskMate-Work');
    await act(async () => {
      fireEvent.click(screen.getByText('开始执行'));
    });
    expect(api.start).toHaveBeenCalledWith('整理截图', 'D:\\DeskMate-Work', 'claude', false, expect.any(Function));
    emit({ type: 'started', task_id: 't1', executor: 'Claude Code' });
    emit({ type: 'step', kind: 'edit', text: '写入 a.txt' });
    expect(screen.getByText(/写入 a.txt/)).toBeTruthy();
    expect(screen.getByText('停止任务')).toBeTruthy();
    emit({ type: 'done', summary: '整理好了', cost_usd: 0.01, changed: [], backup: null });
    expect(screen.getByText('完成了')).toBeTruthy();
    expect(onFinished).toHaveBeenCalledWith('t1');
  });

  it('危险操作需要逐条勾选批准；无法精确放行的不能勾选', async () => {
    const { api, emit } = fakeApi();
    await mount(api);
    await act(async () => {
      fireEvent.click(screen.getByText('开始执行'));
    });
    emit({ type: 'started', task_id: 't1', executor: 'Claude Code' });
    emit({
      type: 'needs_approval',
      summary: '还需要删除一个文件',
      items: [
        { kind: 'command', detail: '执行命令：rm old.log', rule: 'Bash(rm old.log)', path: null },
        { kind: 'command', detail: '执行命令：echo $(x)', rule: null, path: null },
      ],
    });
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
    expect(boxes[1].disabled).toBe(true);
    const approveBtn = screen.getByText('批准并继续') as HTMLButtonElement;
    expect(approveBtn.disabled).toBe(true);
    fireEvent.change(boxes[0], { target: { checked: true } });
    await act(async () => {
      fireEvent.click(approveBtn);
    });
    expect(api.approve).toHaveBeenCalledWith('t1', [0], expect.any(Function));
  });

  it('Codex 修改文件需要明确勾选', async () => {
    const { api } = fakeApi();
    await mount(api);
    fireEvent.change(screen.getByLabelText('执行工具'), { target: { value: 'codex' } });
    expect(screen.getByText(/Codex 默认只读/)).toBeTruthy();
    fireEvent.change(screen.getByRole('checkbox'), { target: { checked: true } });
    await act(async () => {
      fireEvent.click(screen.getByText('开始执行'));
    });
    expect(api.start).toHaveBeenCalledWith('整理截图', 'D:\\DeskMate-Work', 'codex', true, expect.any(Function));
  });
});
