# FinalNotes (read-only) — Claude Desktop extension

Gives Claude read-only access to the FinalNotes folder that the Meeting Notes
app exports to.

`server/index.js` runs the official
[`@modelcontextprotocol/server-filesystem`](https://www.npmjs.com/package/@modelcontextprotocol/server-filesystem)
(pinned in `package-lock.json`) on that one folder and filters its stdio:

- only read tools are listed and callable (allow-list in `server/filter.js`);
  `write_file`, `edit_file`, `move_file`, `create_directory` and any future
  tool are refused;
- MCP roots are stripped, so a client cannot replace the folder with others
  (the bare server lets client roots replace its folders);
- the official server keeps every path inside the folder.

## Install

In the app: Settings > General > FinalNotes export > **Connect Claude Desktop**,
then choose the FinalNotes folder when Claude Desktop asks. The `.mcpb` file is
also uploaded by every Windows CI build (`finalnotes-claude-extension-*`).

## Develop

```bash
npm ci
npm test                      # unit tests + end-to-end against the real server
cd ../../frontend && pnpm run build:mcpb   # writes src-tauri/resources/finalnotes-readonly.mcpb
```

Third-party license: `THIRD_PARTY_LICENSE-mcp-servers.txt`.
