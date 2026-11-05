import {CnnSession} from '@datafog/wasm/cnn';
import {BrowserWorkspace} from '@datafog/wasm/cnn/workspace';
import type {SQLiteWorkspace} from '@datafog/core-sqlite-browser';
import type {FogVaultWorkspace} from '@datafog/core-fogvault-browser';
declare const sqlite: SQLiteWorkspace;
declare const fog: FogVaultWorkspace;
const detector = await CnnSession.open();
BrowserWorkspace.adopt({workspace: sqlite, detector});
BrowserWorkspace.adopt({workspace: fog, detector});
