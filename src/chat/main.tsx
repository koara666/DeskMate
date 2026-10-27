import { render } from 'preact';
import '../styles/base.css';
import '../styles/chat.css';
import { chatApi, taskApi } from '../lib/ipc';
import { ChatApp } from './ChatApp';

render(<ChatApp api={chatApi} taskApi={taskApi} />, document.getElementById('app')!);
