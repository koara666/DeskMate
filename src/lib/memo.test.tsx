import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/preact';
import { memo } from './memo';

describe('memo', () => {
  it('属性没变时不重新渲染，变了才渲染', () => {
    let renders = 0;
    const Item = memo(({ text }: { text: string }) => {
      renders++;
      return <span>{text}</span>;
    });
    const { rerender, container } = render(<Item text="a" />);
    rerender(<Item text="a" />);
    expect(renders).toBe(1);
    rerender(<Item text="b" />);
    expect(renders).toBe(2);
    expect(container.textContent).toBe('b');
  });
});
