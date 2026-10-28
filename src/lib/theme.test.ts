import { describe, expect, it } from 'vitest';
import { applyTheme } from './theme';

describe('角色界面主题', () => {
  it('设皮肤和颜色变量；换角色时清掉旧的；奇怪的值不设', () => {
    const root = document.createElement('div');
    applyTheme({ skin: 'arcade', colors: { accent: '#e8789a', 'accent-2': '#2f5b5c', bad: 'red;x:url(y)' } }, root);
    expect(root.dataset.skin).toBe('arcade');
    expect(root.style.getPropertyValue('--t-accent')).toBe('#e8789a');
    expect(root.style.getPropertyValue('--t-bad')).toBe('');
    applyTheme(undefined, root);
    expect(root.dataset.skin).toBe('default');
    expect(root.style.getPropertyValue('--t-accent')).toBe('');
  });
});
