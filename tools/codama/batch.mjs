// Shows on Mainnet, without signing or sending anything, that the Token program
// runs the instructions packed in a Batch (tag 255): a simulated batch of a
// transfer and an unlimited approval leaves a token account exactly as the same
// two instructions do on their own.
//
//   node tools/codama/batch.mjs [slot]
//
// The token account comes from a recent block: the first SPL Token transfer
// whose authority also pays the fee. Nothing is signed (`sigVerify: false`), so
// nothing here can reach the chain.

import { getAddressDecoder, getAddressEncoder } from '@solana/addresses';

import { fromBase58 } from './display.mjs';

const RPC = process.env.RPC ?? 'https://api.mainnet-beta.solana.com';
const TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
// The "unknown delegate" of the cases: a test key, nobody holds it.
const DELEGATE = 'HeXdWBYaJwLCktf2aD8LcRJUVA3buA3gEvk1zZw4MBJD';
const MAX = 2n ** 64n - 1n;

const toBytes = getAddressEncoder();
const toAddress = getAddressDecoder();

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
            if (json.error) throw Object.assign(new Error(JSON.stringify(json.error)), { final: true });
            return json.result;
        } catch (e) {
            if (e.final) throw e;
            await new Promise(resolve => setTimeout(resolve, 1500 * (attempt + 1)));
        }
    }
    throw new Error(`${method} failed`);
}

const u64 = value => {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setBigUint64(0, value, true);
    return [...bytes];
};

/** An SPL Token account, as far as this check reads it. */
function tokenAccount(data) {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    return {
        amount: view.getBigUint64(64, true),
        delegate: view.getUint32(72, true) === 1 ? toAddress.decode(data.subarray(76, 108)) : null,
        delegatedAmount: view.getBigUint64(121, true),
        owner: toAddress.decode(data.subarray(32, 64)),
    };
}

/** The first transfer of the block that its fee payer authorizes alone. */
async function findTransfer(slot) {
    for (let tries = 0; tries < 40; tries++, slot--) {
        let block;
        try {
            block = await rpc('getBlock', [slot, { encoding: 'json', maxSupportedTransactionVersion: 1, rewards: false, transactionDetails: 'full' }]);
        } catch (e) {
            // -32007 and -32009: a slot that was skipped, or that this node does not keep.
            if (/-3200[79]/.test(e.message)) continue;
            throw e;
        }
        for (const tx of block?.transactions ?? []) {
            if (tx.meta?.err || tx.version !== 'legacy') continue;
            const { accountKeys, header, instructions } = tx.transaction.message;
            if (header.numRequiredSignatures !== 1) continue;
            for (const ix of instructions) {
                if (accountKeys[ix.programIdIndex] !== TOKEN) continue;
                const data = fromBase58(ix.data);
                // Transfer: source, destination, authority. TransferChecked: source, mint, destination, authority.
                const at = data[0] === 3 ? [0, 1, 2] : data[0] === 12 ? [0, 2, 3] : null;
                if (!at || ix.accounts.length !== at[2] + 1 || ix.accounts[at[2]] !== 0) continue;
                const [source, destination] = [accountKeys[ix.accounts[at[0]]], accountKeys[ix.accounts[at[1]]]];
                if (source === destination) continue;
                const now = await rpc('getMultipleAccounts', [[source, destination], { encoding: 'base64' }]);
                const [a, b] = now.value;
                if (!a || !b || a.owner !== TOKEN || b.owner !== TOKEN) continue;
                const state = tokenAccount(Uint8Array.from(Buffer.from(a.data[0], 'base64')));
                if (state.owner !== accountKeys[0] || state.amount < 1n || state.delegate) continue;
                return { destination, owner: accountKeys[0], slot, source, state };
            }
        }
    }
    throw new Error('no transfer found in 40 slots');
}

const { destination, owner, slot, source, state: before } = await findTransfer(
    Number(process.argv[2] ?? (await rpc('getSlot', [{ commitment: 'finalized' }]))),
);

// One fee payer and authority, the two token accounts, then what is only read.
const keys = [owner, source, destination, DELEGATE, TOKEN];
const index = key => keys.indexOf(key);
const transfer = { accounts: [source, destination, owner], data: [3, ...u64(1n)] };
const approve = { accounts: [source, DELEGATE, owner], data: [4, ...u64(MAX)] };
const batch = inner => ({
    accounts: inner.flatMap(ix => ix.accounts),
    data: [255, ...inner.flatMap(ix => [ix.accounts.length, ix.data.length, ...ix.data])],
});

async function simulate(instructions) {
    const message = [1, 0, 2, keys.length, ...keys.flatMap(key => [...toBytes.encode(key)]), ...new Array(32).fill(0), instructions.length];
    for (const ix of instructions) message.push(index(TOKEN), ix.accounts.length, ...ix.accounts.map(index), ix.data.length, ...ix.data);
    const transaction = Buffer.from([1, ...new Array(64).fill(0), ...message]).toString('base64');
    const { value } = await rpc('simulateTransaction', [transaction, {
        accounts: { addresses: [source], encoding: 'base64' },
        encoding: 'base64',
        replaceRecentBlockhash: true,
        sigVerify: false,
    }]);
    const after = value.err || !value.accounts?.[0] ? null : tokenAccount(Uint8Array.from(Buffer.from(value.accounts[0].data[0], 'base64')));
    return { after, err: value.err, units: value.unitsConsumed };
}

const text = s => (s ? `amount ${s.amount}, delegate ${s.delegate ?? 'none'}, delegated amount ${s.delegatedAmount}` : 'no state');
const apart = await simulate([transfer, approve]);
const packed = await simulate([batch([transfer, approve])]);
const alone = await simulate([batch([transfer])]);
const unknown = await simulate([batch([{ accounts: transfer.accounts, data: [200] }])]);
const nested = await simulate([batch([batch([approve])])]);

console.log(`token account ${source} (from slot ${slot})`);
console.log(`before:                         ${text(before)}`);
console.log(`transfer, then approve:         ${apart.err ? JSON.stringify(apart.err) : text(apart.after)} (${apart.units} units)`);
console.log(`one Batch with both:            ${packed.err ? JSON.stringify(packed.err) : text(packed.after)} (${packed.units} units)`);
console.log(`one Batch with the transfer:    ${alone.err ? JSON.stringify(alone.err) : text(alone.after)}`);
console.log(`one Batch with an unknown tag:  ${unknown.err ? JSON.stringify(unknown.err) : 'accepted'}`);
console.log(`a Batch inside a Batch:         ${nested.err ? JSON.stringify(nested.err) : 'accepted'}`);

const same = (a, b) => a && b && a.amount === b.amount && a.delegate === b.delegate && a.delegatedAmount === b.delegatedAmount;
const ok =
    !apart.err && !packed.err && same(apart.after, packed.after) &&
    packed.after.delegate === DELEGATE && packed.after.delegatedAmount === MAX && packed.after.amount === before.amount - 1n &&
    !alone.err && alone.after.delegate === null &&
    Boolean(unknown.err) && Boolean(nested.err);
console.log(ok ? 'the batch does what its instructions do on their own' : 'NOT what was expected');
process.exit(ok ? 0 : 1);
