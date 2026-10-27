import { render } from 'preact';
import '../styles/base.css';
import '../styles/settings.css';
import { settingsApi } from '../lib/ipc';
import { SettingsApp } from './SettingsApp';

render(<SettingsApp api={settingsApi} />, document.getElementById('app')!);
