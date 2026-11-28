// Runs the conformance fixtures of another implementation of the display through
// both Codama releases used here, and says where the answers differ from the ones
// that implementation recorded with the Codama release it is pinned to.
//
// The fixtures are data: a Codama root, instructions, account data, and the display
// expected for each scenario. Their layout is the one of
// github.com/llbartekll/solana-clear-signing-sdk (conformance/fixtures).
//
//   node tools/codama/peers.mjs <fixtures dir> <out.json>
//
// The output has the shape of tests/probes.json, so that the Rust display can be
// run on the same scenarios.

import fs from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { getInstructionDisplay as displayNext } from 'dynamic-instructions-rc';
import { createFromRoot as createNext } from 'codama-rc';
import { upgradeFromJson } from 'codama-upgrade-rc';
import { getInstructionDisplay } from '@codama/dynamic-instructions';
import { AccountRole } from '@solana/instructions';
import { createFromJson } from 'codama';

import { fetchSeveral } from './display.mjs';

const [fixturesDir, outFile] = process.argv.slice(2);
if (!fixturesDir || !outFile) {
    console.error('usage: node tools/codama/peers.mjs <fixtures dir> <out.json>');
    process.exit(2);
}

const roles = {
    readonly: AccountRole.READONLY,
    readonlySigner: AccountRole.READONLY_SIGNER,
    writable: AccountRole.WRITABLE,
    writableSigner: AccountRole.WRITABLE_SIGNER,
};

async function shown(render, root, instruction, options) {
    try {
        return { display: (await render(root, instruction, options)) ?? null };
    } catch (e) {
        return { display: null, error: String(e.message ?? e).split('\n')[0].slice(0, 200) };
    }
}

/** Whether an answer is the one the fixture recorded: the same display, or an error where one is expected. */
const agrees = (recorded, answer) =>
    recorded && typeof recorded === 'object' && 'error' in recorded && !('fields' in recorded)
        ? answer.error !== undefined
        : answer.error === undefined && isDeepStrictEqual(recorded, answer.display);

const out = [];
const sets = fs.readdirSync(fixturesDir, { withFileTypes: true }).filter(e => e.isDirectory()).map(e => e.name).sort();
for (const set of sets) {
    const dir = path.join(fixturesDir, set);
    const text = fs.readFileSync(path.join(dir, 'root.json'), 'utf8');
    const fixture = JSON.parse(fs.readFileSync(path.join(dir, 'cases.json'), 'utf8'));
    const recorded = JSON.parse(fs.readFileSync(path.join(dir, 'expected.json'), 'utf8'));
    const stableRoot = createFromJson(text).getRoot();
    let nextRoot;
    let nextRootError;
    try {
        nextRoot = createNext(upgradeFromJson(text)).getRoot();
    } catch (e) {
        nextRootError = `the IDL does not upgrade: ${String(e.message ?? e).split('\n')[0].slice(0, 160)}`;
    }

    for (const scenario of fixture.scenarios) {
        const accounts = scenario.accounts ?? fixture.accounts;
        const accountData = scenario.accountData ?? fixture.accountData ?? {};
        const instruction = {
            programAddress: scenario.programAddress ?? fixture.programAddress,
            data: Uint8Array.from(Buffer.from(scenario.dataBase64 ?? fixture.dataBase64, 'base64')),
            accounts: accounts.map(a => ({ address: a.address, role: roles[a.role] })),
        };
        // As the fixtures' own oracle does: account data is given only when the scenario asks for it.
        const options = scenario.fetchAccounts
            ? {
                  fetchAccount: async address => {
                      const account = accountData[address];
                      if (!account) return { address, exists: false };
                      const data = Uint8Array.from(Buffer.from(account.dataBase64, 'base64'));
                      return { address, data, executable: false, exists: true, lamports: 0n, programAddress: account.programAddress, space: BigInt(data.length) };
                  },
              }
            : {};
        const stable = await shown(getInstructionDisplay, stableRoot, instruction, options);
        // The release candidate takes account data under another name, several addresses at a time.
        const nextOptions = options.fetchAccount ? { fetchAccounts: fetchSeveral(options.fetchAccount) } : {};
        const next = nextRoot ? await shown(displayNext, nextRoot, instruction, nextOptions) : { display: null, error: nextRootError };
        const fetch = {};
        if (scenario.fetchAccounts) {
            for (const [address, account] of Object.entries(accountData)) fetch[address] = { data: account.dataBase64, owner: account.programAddress };
        }
        out.push({
            name: `${set}/${scenario.name}`,
            set,
            idl: JSON.parse(text),
            program: instruction.programAddress,
            accounts: accounts.map(a => [a.address, a.role.endsWith('Signer')]),
            data: Buffer.from(instruction.data).toString('hex'),
            fetch,
            offline: !scenario.fetchAccounts,
            recorded: recorded[scenario.name] ?? null,
            stable,
            next,
        });
    }
}

fs.writeFileSync(outFile, `[\n${out.map(p => JSON.stringify(p)).join(',\n')}\n]\n`);

const bySet = new Map();
for (const p of out) {
    const row = bySet.get(p.set) ?? { n: 0, stable: 0, next: 0 };
    row.n++;
    if (agrees(p.recorded, p.stable)) row.stable++;
    if (agrees(p.recorded, p.next)) row.next++;
    bySet.set(p.set, row);
}
console.log('set'.padEnd(30), 'scenarios', ' as 1.0.0', ' as 2.0.0-rc.0');
let total = { n: 0, stable: 0, next: 0 };
for (const [set, row] of bySet) {
    console.log(set.padEnd(30), String(row.n).padStart(9), String(row.stable).padStart(9), String(row.next).padStart(14));
    total = { n: total.n + row.n, stable: total.stable + row.stable, next: total.next + row.next };
}
console.log('all'.padEnd(30), String(total.n).padStart(9), String(total.stable).padStart(9), String(total.next).padStart(14));

if (process.argv.includes('--differences')) {
    const brief = a => (a.error ? `error: ${a.error}` : JSON.stringify(a.display));
    for (const p of out) {
        if (agrees(p.recorded, p.stable)) continue;
        console.log(`\n${p.name}\n  recorded: ${JSON.stringify(p.recorded)}\n  1.0.0:    ${brief(p.stable)}`);
    }
}
