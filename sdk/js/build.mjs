// Build plugin.js into a WASM component with componentize-js.
import { componentize } from '@bytecodealliance/componentize-js';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const witPath = fileURLToPath(new URL('../../crates/pluginrt/wit', import.meta.url));
const source = await readFile(new URL('./plugin.js', import.meta.url), 'utf8');

const { component } = await componentize(source, {
  witPath,
  worldName: 'screentools',
  // Drop the JS engine's WASI http/fetch imports — plugins reach the network
  // through the host's gated `host-net`, not ungated WASI, to keep the sandbox.
  disableFeatures: ['http', 'fetch-event'],
});

await writeFile(new URL('./plugin.wasm', import.meta.url), component);
console.log('wrote plugin.wasm:', component.length, 'bytes');
