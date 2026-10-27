import { render } from 'preact';
import '../styles/base.css';

// M2 起逐步实现：模型后端、角色、记忆、用量、任务权限等
function Settings() {
  return (
    <main style={{ padding: '24px' }}>
      <h1>DeskMate 设置</h1>
      <p>设置界面开发中。</p>
    </main>
  );
}

render(<Settings />, document.getElementById('app')!);
