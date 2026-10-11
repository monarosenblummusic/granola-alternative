import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fromClient, fromServer, READ_ONLY_TOOLS } from '../server/filter.js';

test('write tools are refused without reaching the server', () => {
  for (const name of ['write_file', 'edit_file', 'move_file', 'create_directory', 'some_future_tool']) {
    const { forward, reply } = fromClient({ jsonrpc: '2.0', id: 7, method: 'tools/call', params: { name, arguments: {} } });
    assert.equal(forward, undefined);
    assert.equal(reply.id, 7);
    assert.equal(reply.result.isError, true);
    assert.match(reply.result.content[0].text, /read-only/);
  }
});

test('read tools pass through unchanged', () => {
  for (const name of READ_ONLY_TOOLS) {
    const msg = { jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: { path: 'x' } } };
    assert.deepEqual(fromClient(msg), { forward: msg });
  }
});

test('client roots are never advertised or announced', () => {
  const init = {
    jsonrpc: '2.0', id: 0, method: 'initialize',
    params: { protocolVersion: '2025-06-18', capabilities: { roots: { listChanged: true }, sampling: {} }, clientInfo: { name: 'c', version: '1' } },
  };
  const { forward } = fromClient(init);
  assert.deepEqual(forward.params.capabilities, { sampling: {} });
  assert.deepEqual(init.params.capabilities.roots, { listChanged: true }, 'input is not mutated');
  assert.deepEqual(fromClient({ jsonrpc: '2.0', method: 'notifications/roots/list_changed' }), {});
});

test('tool lists only show read tools', () => {
  const tools = ['read_text_file', 'write_file', 'search_files', 'edit_file'].map((name) => ({ name }));
  const { forward } = fromServer({ jsonrpc: '2.0', id: 3, result: { tools, nextCursor: 'n' } });
  assert.deepEqual(forward.result.tools.map((t) => t.name), ['read_text_file', 'search_files']);
  assert.equal(forward.result.nextCursor, 'n');
});

test('server requests for roots are answered with an error', () => {
  const { forward, replyToServer } = fromServer({ jsonrpc: '2.0', id: 9, method: 'roots/list' });
  assert.equal(forward, undefined);
  assert.equal(replyToServer.id, 9);
  assert.equal(replyToServer.error.code, -32601);
});
