#!/usr/bin/env node
// FinalNotes read-only MCP server.
//
// Runs the official @modelcontextprotocol/server-filesystem on one folder (the
// FinalNotes export folder) and sits between it and the client on stdio:
// only read tools are listed or callable, and client "roots" are stripped so
// the folder cannot be swapped for another. The official server itself keeps
// every path inside the folder.
//
// Usage: node server/index.js <FinalNotes folder>

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { fromClient, fromServer } from './filter.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const serverEntry = path.join(here, '..', 'node_modules', '@modelcontextprotocol', 'server-filesystem', 'dist', 'index.js');

function log(message) {
  process.stderr.write(`[finalnotes-readonly] ${message}\n`);
}

/** Expand placeholders a host may leave unresolved in the folder setting. */
function resolveFolder(arg) {
  if (!arg) return null;
  const docs = path.join(os.homedir(), 'Documents');
  const expanded = arg
    .replace(/\$\{DOCUMENTS\}/g, docs)
    .replace(/\$\{HOME\}/g, os.homedir())
    .replace(/^~(?=$|[\\/])/, os.homedir());
  return path.resolve(expanded);
}

const folder = resolveFolder(process.argv[2]);
if (!folder) {
  log('Usage: node server/index.js <FinalNotes folder>');
  process.exit(2);
}
try {
  fs.mkdirSync(folder, { recursive: true });
} catch (err) {
  log(`Cannot use ${folder}: ${err.message}`);
  process.exit(2);
}
if (!fs.existsSync(serverEntry)) {
  log(`Official filesystem server not found at ${serverEntry}`);
  process.exit(2);
}

const child = spawn(process.execPath, [serverEntry, folder], { stdio: ['pipe', 'pipe', 'inherit'] });

const send = (stream, msg) => stream.write(`${JSON.stringify(msg)}\n`);

function relay(line, filter, onForward, onReply) {
  if (!line.trim()) return;
  let parsed;
  try {
    parsed = JSON.parse(line);
  } catch {
    log('Dropped a message that is not valid JSON');
    return;
  }
  const messages = Array.isArray(parsed) ? parsed : [parsed];
  for (const msg of messages) {
    const result = filter(msg);
    if (result.forward !== undefined) onForward(result.forward);
    const reply = result.reply ?? result.replyToServer;
    if (reply !== undefined) onReply(reply);
  }
}

createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', (line) =>
  relay(line, fromClient, (m) => send(child.stdin, m), (m) => send(process.stdout, m)),
);
createInterface({ input: child.stdout, crlfDelay: Infinity }).on('line', (line) =>
  relay(line, fromServer, (m) => send(process.stdout, m), (m) => send(child.stdin, m)),
);

process.stdin.on('end', () => child.stdin.end());
child.on('exit', (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
child.on('error', (err) => {
  log(`Could not start the filesystem server: ${err.message}`);
  process.exit(1);
});
for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => child.kill(sig));
