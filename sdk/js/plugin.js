// Prompt WASM plugin — JavaScript authoring template. Implemented against the
// same WIT world as the Rust template, built to a component with componentize-js
// (`npm run build`) — so it ships a self-contained .wasm with no runtime
// dependency. The `guest` export provides the interface the host calls; host
// functions are imported from their `prompt:plugin/*` interfaces.

import { log } from 'prompt:plugin/host-core@0.1.0';
import { readScreen } from 'prompt:plugin/host-screen@0.1.0';

export const guest = {
  init() {
    log('info', 'js plugin ready');
  },

  // A tool call. Return a JSON result string; throw a string on error.
  callTool(name, paramsJson) {
    if (name !== 'wordcount') {
      throw `unknown tool: ${name}`;
    }
    const params = JSON.parse(paramsJson || '{}');
    const lines = params.lines ?? 200;
    const screen = readScreen(lines); // gated host call (needs `screen` capability)
    const words = screen.split(/\s+/).filter(Boolean).length;
    return JSON.stringify({ words });
  },

  // A panel node tree ({ title, blocks: [...] }); "{}" for a tool-only plugin.
  render(_requestJson) {
    return JSON.stringify({
      title: 'JS Plugin',
      blocks: [{ type: 'text', text: 'Built with componentize-js.' }],
    });
  },

  onUiEvent(_eventJson) {},
};
