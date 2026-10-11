// Runs the real proxy + official filesystem server and talks to it with the
// official MCP client, the same way Claude Desktop does (stdio).
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { ListRootsRequestSchema } from '@modelcontextprotocol/sdk/types.js';

const proxy = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'server', 'index.js');
const base = fs.mkdtempSync(path.join(os.tmpdir(), 'finalnotes-e2e-'));
const notes = path.join(base, 'FinalNotes');
const outside = path.join(base, 'Private');
const meetingFile = path.join(notes, '2026-10-11 1405 Standup [1a2b3c4d].md');
const secretFile = path.join(outside, 'secret.txt');
let client;

const text = (result) => result.content.map((c) => c.text ?? '').join('\n');

before(async () => {
  fs.mkdirSync(notes, { recursive: true });
  fs.mkdirSync(outside, { recursive: true });
  fs.writeFileSync(meetingFile, '---\ntitle: "Standup"\n---\n\n# Standup\n\n## AI notes\n\n- Release is on Friday\n');
  fs.writeFileSync(secretFile, 'do not read');

  // A client that offers roots, to check they cannot widen access.
  client = new Client({ name: 'e2e', version: '1.0.0' }, { capabilities: { roots: { listChanged: true } } });
  client.setRequestHandler(ListRootsRequestSchema, async () => ({ roots: [{ uri: pathToFileURL(outside).href }] }));
  await client.connect(new StdioClientTransport({ command: process.execPath, args: [proxy, notes], stderr: 'ignore' }));
});

after(async () => {
  await client?.close();
  fs.rmSync(base, { recursive: true, force: true });
});

test('only read tools are listed', async () => {
  const names = (await client.listTools()).tools.map((t) => t.name).sort();
  assert.deepEqual(names, [
    'directory_tree', 'get_file_info', 'list_allowed_directories', 'list_directory', 'list_directory_with_sizes',
    'read_file', 'read_media_file', 'read_multiple_files', 'read_text_file', 'search_files',
  ]);
});

test('meeting notes can be listed, searched and read', async () => {
  assert.match(text(await client.callTool({ name: 'list_directory', arguments: { path: notes } })), /Standup \[1a2b3c4d\]\.md/);
  assert.match(text(await client.callTool({ name: 'search_files', arguments: { path: notes, pattern: '*Standup*' } })), /Standup/);
  assert.match(text(await client.callTool({ name: 'read_text_file', arguments: { path: meetingFile } })), /Release is on Friday/);
});

test('every write tool is refused and nothing changes on disk', async () => {
  const before = fs.readdirSync(notes).sort();
  const attempts = [
    ['write_file', { path: path.join(notes, 'new.md'), content: 'x' }],
    ['edit_file', { path: meetingFile, edits: [{ oldText: 'Friday', newText: 'Monday' }] }],
    ['move_file', { source: meetingFile, destination: path.join(notes, 'moved.md') }],
    ['create_directory', { path: path.join(notes, 'sub') }],
  ];
  for (const [name, args] of attempts) {
    const result = await client.callTool({ name, arguments: args });
    assert.equal(result.isError, true, name);
    assert.match(text(result), /read-only/, name);
  }
  assert.deepEqual(fs.readdirSync(notes).sort(), before);
  assert.match(fs.readFileSync(meetingFile, 'utf8'), /Friday/);
});

test('files outside FinalNotes cannot be read, even after the client announces roots', async () => {
  await client.sendRootsListChanged();
  await new Promise((r) => setTimeout(r, 300));
  const result = await client.callTool({ name: 'read_text_file', arguments: { path: secretFile } });
  assert.equal(result.isError, true);
  assert.doesNotMatch(text(result), /do not read/);
  const allowed = text(await client.callTool({ name: 'list_allowed_directories', arguments: {} }));
  assert.match(allowed, /FinalNotes/);
  assert.doesNotMatch(allowed, /Private/);
});

test('".." paths stay inside the folder', async () => {
  const sneaky = [notes, '..', 'Private', 'secret.txt'].join(path.sep); // not normalised on purpose
  const result = await client.callTool({ name: 'read_text_file', arguments: { path: sneaky } });
  assert.equal(result.isError, true);
  assert.doesNotMatch(text(result), /do not read/);
});

test('symlinks out of the folder are refused', { skip: process.platform === 'win32' && 'needs symlink rights on Windows' }, async () => {
  const link = path.join(notes, 'link.txt');
  fs.symlinkSync(secretFile, link);
  try {
    const result = await client.callTool({ name: 'read_text_file', arguments: { path: link } });
    assert.equal(result.isError, true);
    assert.doesNotMatch(text(result), /do not read/);
  } finally {
    fs.unlinkSync(link);
  }
});
