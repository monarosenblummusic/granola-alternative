// Fork: build the FinalNotes read-only Claude Desktop extension (.mcpb) from
// mcp/finalnotes-readonly into src-tauri/resources so it ships with the app.
// Production dependencies only; the official filesystem server is pinned in
// mcp/finalnotes-readonly/package-lock.json.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const MCPB_CLI = '@anthropic-ai/mcpb@2.1.2';
const frontend = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const source = path.resolve(frontend, '..', 'mcp', 'finalnotes-readonly');
const staging = path.join(source, '.build');
const output = path.join(frontend, 'src-tauri', 'resources', 'finalnotes-readonly.mcpb');

function run(cmd, args, cwd) {
  // npm/npx are .cmd shims on Windows and need a shell there, so quote paths.
  const shell = process.platform === 'win32';
  const quoted = shell ? args.map((a) => (/[\s"]/.test(a) ? `"${a}"` : a)) : args;
  const result = spawnSync(cmd, quoted, { cwd, stdio: 'inherit', shell });
  if (result.status !== 0) {
    console.error(`${cmd} ${args.join(' ')} failed`);
    process.exit(result.status ?? 1);
  }
}

fs.rmSync(staging, { recursive: true, force: true });
fs.mkdirSync(staging, { recursive: true });
for (const item of ['manifest.json', 'package.json', 'package-lock.json', '.mcpbignore', 'server', 'THIRD_PARTY_LICENSE-mcp-servers.txt']) {
  fs.cpSync(path.join(source, item), path.join(staging, item), { recursive: true });
}
run('npm', ['ci', '--omit=dev', '--no-audit', '--no-fund'], staging);
fs.mkdirSync(path.dirname(output), { recursive: true });
run('npx', ['--yes', MCPB_CLI, 'pack', '.', output], staging);
fs.rmSync(staging, { recursive: true, force: true });
console.log(`Built ${output}`);
