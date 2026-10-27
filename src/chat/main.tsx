import { render } from 'preact';
import '../styles/base.css';
import '../styles/chat.css';
import { chatApi } from '../lib/ipc';
import { ChatApp } from './ChatApp';

render(<ChatApp api={chatApi} />, document.getElementById('app')!);
