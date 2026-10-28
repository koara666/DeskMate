import { render } from 'preact';
import '../styles/base.css';
import '../styles/pet.css';
import '../styles/skins/arcade.css';
import { petApi, voiceApi } from '../lib/ipc';
import { Pet } from './Pet';

render(<Pet api={petApi} voice={voiceApi} />, document.getElementById('app')!);
