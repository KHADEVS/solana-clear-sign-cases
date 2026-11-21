// Converts the Anchor IDLs in idl/anchor/source/ to Codama IDLs in idl/anchor/,
// with Codama's own converter (@codama/nodes-from-anchor). Anchor IDLs carry no
// display metadata, so these show what Codama displays for a program that has
// not added any.
//
//   node tools/codama/from-anchor.mjs           write idl/anchor/*.json
//   node tools/codama/from-anchor.mjs --check   fail if a conversion changed

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createFromRoot } from 'codama';
import { rootNodeFromAnchor } from '@codama/nodes-from-anchor';

const here = path.dirname(fileURLToPath(import.meta.url));
const dir = path.resolve(here, '../../idl/anchor');
const check = process.argv.includes('--check');
let failed = 0;
for (const name of fs.readdirSync(path.join(dir, 'source')).sort()) {
    if (!name.endsWith('.json')) continue;
    const anchor = JSON.parse(fs.readFileSync(path.join(dir, 'source', name), 'utf8'));
    const json = JSON.stringify(JSON.parse(createFromRoot(rootNodeFromAnchor(anchor)).getJson()), null, 2) + '\n';
    const out = path.join(dir, name);
    if (check) {
        const ok = fs.existsSync(out) && fs.readFileSync(out, 'utf8') === json;
        console.log(`${ok ? 'PASS' : 'FAIL'} anchor/${name}`);
        if (!ok) failed++;
    } else {
        fs.writeFileSync(out, json);
        console.log(`converted anchor/${name}`);
    }
}
if (check) process.exitCode = failed ? 1 : 0;
