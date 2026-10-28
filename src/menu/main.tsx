import { render } from 'preact';
import '../styles/base.css';
import '../styles/menu.css';
import { menuApi } from '../lib/ipc';
import { Menu } from './Menu';

render(<Menu api={menuApi} />, document.getElementById('app')!);
