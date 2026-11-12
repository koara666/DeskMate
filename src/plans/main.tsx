import { render } from 'preact';
import '../styles/base.css';
import '../styles/settings.css';
import '../styles/plans.css';
import { plansApi } from '../lib/ipc';
import { PlansApp } from './PlansApp';

render(<PlansApp api={plansApi} />, document.getElementById('app')!);
