// Downloads the conformance fixtures of another implementation at a fixed commit.
// They are data files (a Codama root, instructions, account data, expected displays);
// nothing of that repository is run.
//
//   node tools/codama/peers-fetch.mjs <owner/repo> <commit> <out dir> [path prefix]
//
// The prefix defaults to conformance/fixtures/, the layout of
// github.com/llbartekll/solana-clear-signing-sdk.

import fs from 'node:fs';
import path from 'node:path';

const [repo, commit, out, prefix = 'conformance/fixtures/'] = process.argv.slice(2);
if (!repo || !commit || !out) {
    console.error('usage: node tools/codama/peers-fetch.mjs <owner/repo> <commit> <out dir> [path prefix]');
    process.exit(2);
}

const tree = await fetch(`https://api.github.com/repos/${repo}/git/trees/${commit}?recursive=1`, {
    headers: { accept: 'application/vnd.github+json', 'user-agent': 'clear-sign-cases' },
});
if (!tree.ok) throw new Error(`the file list of ${repo} at ${commit}: HTTP ${tree.status}`);
const files = (await tree.json()).tree.filter(e => e.type === 'blob' && e.path.startsWith(prefix) && e.path.endsWith('.json'));

for (const { path: file } of files) {
    const target = path.join(out, file.slice(prefix.length));
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const response = await fetch(`https://raw.githubusercontent.com/${repo}/${commit}/${file}`);
    if (!response.ok) throw new Error(`${file}: HTTP ${response.status}`);
    fs.writeFileSync(target, Buffer.from(await response.arrayBuffer()));
}
console.log(`${files.length} files of ${repo} at ${commit.slice(0, 12)} written to ${out}`);
