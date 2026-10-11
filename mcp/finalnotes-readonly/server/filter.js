// Read-only filter for the official MCP filesystem server.
//
// Pure functions over JSON-RPC messages, used by index.js and the tests.
// Tools are allow-listed, so any write tool added to a future server version
// stays blocked.

export const READ_ONLY_TOOLS = new Set([
  'read_file', // deprecated alias of read_text_file
  'read_text_file',
  'read_media_file',
  'read_multiple_files',
  'list_directory',
  'list_directory_with_sizes',
  'directory_tree',
  'search_files',
  'get_file_info',
  'list_allowed_directories',
]);

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/**
 * A message from the client (Claude) to the server.
 * Returns { forward?: message, reply?: message }.
 */
export function fromClient(msg) {
  if (!isObject(msg)) return { forward: msg };

  if (msg.method === 'initialize' && isObject(msg.params?.capabilities) && 'roots' in msg.params.capabilities) {
    // With client roots the server would replace the FinalNotes folder with
    // whatever folders the client offers, so roots are never advertised.
    const { roots: _ignored, ...capabilities } = msg.params.capabilities;
    return { forward: { ...msg, params: { ...msg.params, capabilities } } };
  }

  if (msg.method === 'notifications/roots/list_changed') return {};

  if (msg.method === 'tools/call') {
    const name = msg.params?.name;
    if (!READ_ONLY_TOOLS.has(name)) {
      if (msg.id === undefined) return {};
      return {
        reply: {
          jsonrpc: '2.0',
          id: msg.id,
          result: {
            content: [{ type: 'text', text: `"${name}" is not available: FinalNotes access is read-only.` }],
            isError: true,
          },
        },
      };
    }
  }

  return { forward: msg };
}

/**
 * A message from the server to the client.
 * Returns { forward?: message, replyToServer?: message }.
 */
export function fromServer(msg) {
  if (!isObject(msg)) return { forward: msg };

  if (msg.method === 'roots/list' && msg.id !== undefined) {
    return {
      replyToServer: { jsonrpc: '2.0', id: msg.id, error: { code: -32601, message: 'Roots are not supported' } },
    };
  }

  if (isObject(msg.result) && Array.isArray(msg.result.tools)) {
    const tools = msg.result.tools.filter((t) => READ_ONLY_TOOLS.has(t?.name));
    return { forward: { ...msg, result: { ...msg.result, tools } } };
  }

  return { forward: msg };
}
