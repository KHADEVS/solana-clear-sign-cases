// Fills each case's expected displays with Codama's own instruction display:
// `expected.display` from the stable release, `expected.displayNext` from the
// 2.0 release candidate (IDLs upgraded from 1.x with @codama/upgrade).
//
//   node tools/codama/generate.mjs           write them into cases/*/case.json
//   node tools/codama/generate.mjs --check   fail if a case no longer matches

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { getInstructionDisplay as displayNext } from 'dynamic-instructions-rc';
import { createFromRoot as createNext } from 'codama-rc';
import { upgradeFromJson } from 'codama-upgrade-rc';

import { decodeMessage, displayInstructions, loadRoots, makeFetchAccount } from './display.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
// The versions actually installed (package exports hide package.json from require).
const version = name =>
    JSON.parse(fs.readFileSync(path.join(here, 'node_modules', name, 'package.json'), 'utf8')).version;
const stable = `@codama/dynamic-instructions ${version('@codama/dynamic-instructions')} (codama ${version('codama')})`;
const next = `@codama/dynamic-instructions ${version('dynamic-instructions-rc')} (codama ${version('codama-rc')}, IDLs upgraded by @codama/upgrade ${version('codama-upgrade-rc')})`;

async function displaysNext(instructions, idlTexts, fetchAccount) {
    const roots = new Map();
    for (const text of idlTexts) {
        const node = createNext(upgradeFromJson(text)).getRoot();
        roots.set(node.program.publicKey, node);
    }
    const out = [];
    for (const instruction of instructions) {
        const programRoot = roots.get(instruction.programAddress);
        let display = null;
        let error;
        if (programRoot) {
            try {
                display = await displayNext(programRoot, instruction, { fetchAccount });
            } catch (e) {
                error = String(e.message ?? e);
            }
        }
        out.push({ display, program: instruction.programAddress, ...(error ? { error } : {}) });
    }
    return out;
}

const check = process.argv.includes('--check');
let failed = 0;
const casesDir = path.join(root, 'cases');
for (const id of fs.readdirSync(casesDir).sort()) {
    const file = path.join(casesDir, id, 'case.json');
    if (!fs.existsSync(file)) continue;
    const testCase = JSON.parse(fs.readFileSync(file, 'utf8'));
    const message = decodeMessage(Uint8Array.from(Buffer.from(testCase.message, 'base64')), testCase.lookupTables ?? {});
    const idlTexts = testCase.idls.map(name => fs.readFileSync(path.join(root, 'idl', name), 'utf8'));
    const fetchAccount = makeFetchAccount(testCase.context?.accounts ?? {});
    const display = {
        generator: stable,
        instructions: await displayInstructions(message.instructions, loadRoots(idlTexts), fetchAccount),
    };
    const displayNextRun = {
        generator: next,
        instructions: await displaysNext(message.instructions, idlTexts, fetchAccount),
    };
    if (check) {
        const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
        const ok = same(testCase.expected.display, display) && same(testCase.expected.displayNext, displayNextRun);
        console.log(`${ok ? 'PASS' : 'FAIL'} ${id}`);
        if (!ok) failed++;
    } else {
        testCase.expected = { display, displayNext: displayNextRun, ...testCase.expected };
        testCase.expected.display = display;
        testCase.expected.displayNext = displayNextRun;
        fs.writeFileSync(file, JSON.stringify(testCase, null, 2) + '\n');
        console.log(`filled ${id}`);
    }
}
if (check) {
    console.log(failed ? `${failed} cases no longer match` : 'every case matches');
    process.exitCode = failed ? 1 : 0;
}
