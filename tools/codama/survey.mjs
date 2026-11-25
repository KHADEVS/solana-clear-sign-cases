// Takes Mainnet blocks, and for every instruction of a program that has an IDL
// here (top-level and inner), records what Codama displays for it, with both
// releases. `cargo run --bin compare` then renders the same instructions with
// the Rust implementation and reports every difference.
//
//   node tools/codama/survey.mjs <blocks> [slots between two blocks] [out dir]
//   node tools/codama/survey.mjs --slots <slot>,<slot>,... [out dir]

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';

import { getInstructionDisplay as displayNext } from 'dynamic-instructions-rc';
import { createFromRoot as createNext } from 'codama-rc';
import { upgradeFromJson } from 'codama-upgrade-rc';
import { getInstructionDisplay } from '@codama/dynamic-instructions';
import { getAddressDecoder } from '@solana/addresses';
import { AccountRole } from '@solana/instructions';

import { byProgram, fromBase58, loadRoots } from './display.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const args = process.argv.slice(2);
// Either the slots to take, or a number of blocks counted back from the last finalized one.
const given = args[0] === '--slots' ? args.splice(0, 2)[1].split(',').map(Number) : null;
const blocks = given ? given.length : Number(args.shift() ?? 4);
const spacing = given ? 0 : Number(args.shift() ?? 9000);
const outDir = path.resolve(args.shift() ?? path.join(root, 'real/survey'));
const RPC = process.env.RPC ?? 'https://api.mainnet-beta.solana.com';
const IDLS = ['official/system.json', 'official/token.json', 'official/token-2022.json', 'official/associated-token-account.json',
    'official/compute-budget.json', 'official/stake.json', 'official/memo.json', 'anchor/jupiter.json'];
const VOTE = 'Vote111111111111111111111111111111111111111';

const address = getAddressDecoder();

async function rpc(method, params) {
    for (let attempt = 0; attempt < 8; attempt++) {
        try {
            const response = await fetch(RPC, {
                body: JSON.stringify({ id: 1, jsonrpc: '2.0', method, params }),
                headers: { 'content-type': 'application/json' },
                method: 'POST',
                signal: AbortSignal.timeout(60000),
            });
            if (response.status === 429) throw new Error('rate limited');
            const json = await response.json();
            if (json.error) return { error: json.error };
            return { result: json.result };
        } catch {
            await new Promise(resolve => setTimeout(resolve, 1500 * (attempt + 1)));
        }
    }
    throw new Error(`${method} failed`);
}

/** The compiled instructions and static keys of a transaction, whatever its version. */
function parseTransaction(bytes) {
    let pos = 0;
    const u8 = () => bytes[pos++];
    const take = n => {
        if (pos + n > bytes.length) throw new Error('transaction ends too early');
        pos += n;
        return bytes.subarray(pos - n, pos);
    };
    const compact = () => {
        let value = 0;
        for (let shift = 0; shift <= 14; shift += 7) {
            const byte = u8();
            value |= (byte & 0x7f) << shift;
            if (!(byte & 0x80)) return value;
        }
        throw new Error('compact-u16 longer than 3 bytes');
    };
    // A v1 transaction starts with its message; the signatures come last.
    const v1 = bytes[0] === 0x81;
    if (!v1) take(64 * compact());
    let first = u8();
    let version = null;
    if (first & 0x80) {
        version = first & 0x7f;
        first = u8();
    }
    const header = { readonlySigned: u8(), readonlyUnsigned: 0, signers: first };
    header.readonlyUnsigned = u8();
    let keys;
    let compiled;
    if (version === 1) {
        const mask = new DataView(Uint8Array.from(take(4)).buffer).getUint32(0, true);
        take(32);
        const [instructionCount, addressCount] = take(2);
        keys = Array.from({ length: addressCount }, () => address.decode(take(32)));
        if (mask & 0b11) take(8);
        for (const bit of [4, 8, 16]) if (mask & bit) take(4);
        const headers = Array.from({ length: instructionCount }, () => {
            const [program, accounts, lo, hi] = take(4);
            return { accounts, dataLength: lo | (hi << 8), program };
        });
        compiled = headers.map(h => ({ accounts: [...take(h.accounts)], data: Uint8Array.from(take(h.dataLength)), program: h.program }));
        if (bytes.length - pos !== 64 * header.signers) throw new Error('v1 transaction of an unexpected length');
    } else {
        keys = Array.from({ length: compact() }, () => address.decode(take(32)));
        take(32);
        compiled = Array.from({ length: compact() }, () => {
            const program = u8();
            const accounts = [...take(compact())];
            return { accounts, data: Uint8Array.from(take(compact())), program };
        });
    }
    return { compiled, header, keys, version };
}

const idlTexts = IDLS.map(name => fs.readFileSync(path.join(root, 'idl', name), 'utf8'));
const stableRoots = loadRoots(idlTexts);
const nextRoots = byProgram(idlTexts.map(text => createNext(upgradeFromJson(text)).getRoot()));

async function shown(render, programRoot, instruction, fetchAccount) {
    try {
        return { display: await render(programRoot, instruction, { fetchAccount }) };
    } catch (e) {
        return { display: null, error: String(e.message ?? e).slice(0, 200) };
    }
}

const head = given ? 0 : (await rpc('getSlot', [{ commitment: 'finalized' }])).result;
const records = [];
const summary = { blocks: [], failedToParse: 0, transactions: 0, votes: 0 };
for (let n = 0; n < blocks; n++) {
    let slot = given ? given[n] : head - n * spacing;
    let block;
    // A slot counted back may have been skipped: the ones before it are tried. A given slot is taken or not.
    for (let tries = 0; tries < (given ? 1 : 20) && !block; tries++, slot--) {
        const answer = await rpc('getBlock', [slot, { encoding: 'base64', maxSupportedTransactionVersion: 1, rewards: false, transactionDetails: 'full' }]);
        if (answer.result) block = answer.result;
        else if (tries === 0 && answer.error?.code === -32602) throw new Error(JSON.stringify(answer.error));
    }
    if (!block) {
        if (given) throw new Error(`no block at slot ${given[n]}`);
        continue;
    }
    slot++;
    let kept = 0;
    for (const [index, tx] of block.transactions.entries()) {
        summary.transactions++;
        let parsed;
        try {
            parsed = parseTransaction(Uint8Array.from(Buffer.from(tx.transaction[0], 'base64')));
        } catch {
            summary.failedToParse++;
            continue;
        }
        const loaded = tx.meta?.loadedAddresses ?? { readonly: [], writable: [] };
        const keys = [...parsed.keys, ...loaded.writable, ...loaded.readonly];
        const { signers, readonlySigned, readonlyUnsigned } = parsed.header;
        const statics = parsed.keys.length;
        const role = i => {
            if (i < signers) return i < signers - readonlySigned ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER;
            if (i < statics) return i < statics - readonlyUnsigned ? AccountRole.WRITABLE : AccountRole.READONLY;
            return i < statics + loaded.writable.length ? AccountRole.WRITABLE : AccountRole.READONLY;
        };
        if (parsed.compiled.every(ix => keys[ix.program] === VOTE)) {
            summary.votes++;
            continue;
        }
        const inner = (tx.meta?.innerInstructions ?? []).flatMap(group =>
            group.instructions.map(ix => ({ accounts: ix.accounts, data: fromBase58(ix.data), inner: true, program: ix.programIdIndex })),
        );
        for (const ix of [...parsed.compiled, ...inner]) {
            const program = keys[ix.program];
            if (!stableRoots.has(program) || ix.accounts.some(i => i >= keys.length)) continue;
            records.push({
                accounts: ix.accounts.map(i => ({ address: keys[i], role: role(i) })),
                data: ix.data,
                inner: Boolean(ix.inner),
                program,
                slot,
                tx: index,
                version: parsed.version,
            });
            kept++;
        }
    }
    summary.blocks.push({ instructions: kept, slot, time: block.blockTime, transactions: block.transactions.length });
    console.log(`slot ${slot}: ${block.transactions.length} transactions, ${kept} instructions kept`);
}

// A first pass tells which accounts the displays read; they are fetched once, as they are now.
const wanted = new Set();
const noting = async a => (wanted.add(a), { address: a, exists: false });
for (const r of records) {
    const instruction = { accounts: r.accounts, data: r.data, programAddress: r.program };
    await shown(getInstructionDisplay, stableRoots.get(r.program), instruction, noting);
}
const accounts = {};
const list = [...wanted];
for (let i = 0; i < list.length; i += 100) {
    const answer = await rpc('getMultipleAccounts', [list.slice(i, i + 100), { encoding: 'base64' }]);
    (answer.result?.value ?? []).forEach((account, j) => {
        accounts[list[i + j]] = account ? { data: account.data[0], owner: account.owner } : null;
    });
}
const fetchAccount = async a => {
    const entry = accounts[a];
    if (!entry) return { address: a, exists: false };
    const data = Uint8Array.from(Buffer.from(entry.data, 'base64'));
    return { address: a, data, executable: false, exists: true, lamports: 0n, programAddress: entry.owner, space: BigInt(data.length) };
};

fs.mkdirSync(outDir, { recursive: true });
const lines = [];
for (const r of records) {
    const instruction = { accounts: r.accounts, data: r.data, programAddress: r.program };
    lines.push(JSON.stringify({
        accounts: r.accounts.map(a => [a.address, a.role >= 2]),
        data: Buffer.from(r.data).toString('base64'),
        inner: r.inner,
        next: await shown(displayNext, nextRoots.get(r.program), instruction, fetchAccount),
        program: r.program,
        slot: r.slot,
        stable: await shown(getInstructionDisplay, stableRoots.get(r.program), instruction, fetchAccount),
        tx: r.tx,
        version: r.version,
    }));
}
fs.writeFileSync(path.join(outDir, 'instructions.jsonl.gz'), zlib.gzipSync(lines.join('\n') + '\n', { level: 9 }));
fs.writeFileSync(path.join(outDir, 'accounts.json'), JSON.stringify(accounts, null, 1) + '\n');
fs.writeFileSync(path.join(outDir, 'summary.json'), JSON.stringify({ ...summary, accounts: list.length, idls: IDLS, instructions: records.length }, null, 1) + '\n');
console.log(JSON.stringify({ ...summary, blocks: summary.blocks.length, accounts: list.length, instructions: records.length }));
