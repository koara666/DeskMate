import { render } from 'preact';
import '../styles/base.css';
import '../styles/pet.css';
import { petApi } from '../lib/ipc';
import { Pet } from './Pet';

render(<Pet api={petApi} />, document.getElementById('app')!);
