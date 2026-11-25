// Codama's own instruction display for a Solana message: decodes the message
// (legacy, v0 or v1), finds each instruction's program in the given Codama
// IDLs, and returns what @codama/dynamic-instructions shows for it.

import { getInstructionDisplay } from '@codama/dynamic-instructions';
import { getAddressDecoder } from '@solana/addresses';
import { AccountRole } from '@solana/instructions';
import { createFromJson } from 'codama';

const addressDecoder = getAddressDecoder();

/** Decodes a message into instructions with their account roles. */
export function decodeMessage(bytes, lookupTables = {}) {
    let pos = 0;
    const u8 = () => {
        if (pos >= bytes.length) throw new Error('message ends too early');
        return bytes[pos++];
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
    const key = () => {
        const out = addressDecoder.decode(bytes.slice(pos, pos + 32));
        pos += 32;
        return out;
    };
    let first = u8();
    let version = null;
    if (first & 0x80) {
        version = first & 0x7f;
        if (version > 1) throw new Error(`unsupported message version ${version}`);
        first = u8();
    }
    const [signers, readonlySigned, readonlyUnsigned] = [first, u8(), u8()];
    if (version === 1) return decodeV1Body(bytes, pos, { signers, readonlySigned, readonlyUnsigned });
    const staticKeys = Array.from({ length: compact() }, key);
    pos += 32; // recent blockhash or nonce
    const compiled = Array.from({ length: compact() }, () => {
        const program = u8();
        const accounts = Array.from({ length: compact() }, u8);
        const length = compact();
        const data = bytes.slice(pos, pos + length);
        pos += length;
        return { accounts, data, program };
    });
    const loadedWritable = [];
    const loadedReadonly = [];
    if (version !== null) {
        for (let n = compact(); n > 0; n--) {
            const table = key();
            const addresses = lookupTables[table];
            if (!addresses) throw new Error(`lookup table ${table} not provided`);
            for (let i = compact(); i > 0; i--) loadedWritable.push(addresses[u8()]);
            for (let i = compact(); i > 0; i--) loadedReadonly.push(addresses[u8()]);
        }
    }
    if (pos !== bytes.length) throw new Error('trailing bytes after the message');
    const keys = [...staticKeys, ...loadedWritable, ...loadedReadonly];
    const role = index => {
        if (index < signers) {
            return index < signers - readonlySigned ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER;
        }
        if (index < staticKeys.length) {
            return index < staticKeys.length - readonlyUnsigned ? AccountRole.WRITABLE : AccountRole.READONLY;
        }
        return index < staticKeys.length + loadedWritable.length ? AccountRole.WRITABLE : AccountRole.READONLY;
    };
    return {
        instructions: compiled.map(ix => ({
            accounts: ix.accounts.map(i => ({ address: keys[i], role: role(i) })),
            data: ix.data,
            programAddress: keys[ix.program],
        })),
        signers: staticKeys.slice(0, signers),
        version,
    };
}

/**
 * The rest of a v1 message (SIMD-0385), from just after its legacy header:
 * config mask, lifetime, counts, addresses, config values, instruction
 * headers, then each instruction's accounts and data. No lookup tables.
 */
function decodeV1Body(bytes, start, header) {
    let pos = start;
    const take = n => {
        if (pos + n > bytes.length) throw new Error('message ends too early');
        const out = bytes.slice(pos, pos + n);
        pos += n;
        return out;
    };
    const mask = new DataView(Uint8Array.from(take(4)).buffer).getUint32(0, true);
    if (mask & ~0b11111) throw new Error(`unknown config bits ${mask}`);
    if ((mask & 0b11) === 0b01 || (mask & 0b11) === 0b10) throw new Error('the priority fee takes both config bits 0 and 1');
    take(32); // lifetime specifier (recent blockhash or nonce)
    const [instructionCount, addressCount] = take(2);
    const keys = Array.from({ length: addressCount }, () => addressDecoder.decode(take(32)));
    const config = {};
    if (mask & 0b11) config.priorityFee = new DataView(Uint8Array.from(take(8)).buffer).getBigUint64(0, true);
    for (const [bit, name] of [[4, 'computeUnitLimit'], [8, 'loadedAccountsDataSizeLimit'], [16, 'heapSize']]) {
        if (mask & bit) config[name] = new DataView(Uint8Array.from(take(4)).buffer).getUint32(0, true);
    }
    const headers = Array.from({ length: instructionCount }, () => {
        const [program, accounts, lo, hi] = take(4);
        return { accounts, dataLength: lo | (hi << 8), program };
    });
    const compiled = headers.map(h => ({ accounts: [...take(h.accounts)], data: take(h.dataLength), program: h.program }));
    if (pos !== bytes.length) throw new Error('trailing bytes after the message');
    const { signers, readonlySigned, readonlyUnsigned } = header;
    const role = index => {
        if (index < signers) {
            return index < signers - readonlySigned ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER;
        }
        return index < keys.length - readonlyUnsigned ? AccountRole.WRITABLE : AccountRole.READONLY;
    };
    return {
        config,
        instructions: compiled.map(ix => ({
            accounts: ix.accounts.map(i => {
                if (i >= keys.length) throw new Error(`account index ${i} out of range`);
                return { address: keys[i], role: role(i) };
            }),
            data: ix.data,
            programAddress: keys[ix.program],
        })),
        signers: keys.slice(0, signers),
        version: 1,
    };
}

const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

/** The bytes of a base58 text, as the RPC gives the data of inner instructions. */
export function fromBase58(text) {
    let value = 0n;
    for (const character of text) {
        const digit = BASE58.indexOf(character);
        if (digit < 0) throw new Error(`not base58: ${text}`);
        value = value * 58n + BigInt(digit);
    }
    const bytes = [];
    for (; value > 0n; value >>= 8n) bytes.unshift(Number(value & 0xffn));
    for (const character of text) {
        if (character !== '1') break;
        bytes.unshift(0);
    }
    return Uint8Array.from(bytes);
}

/**
 * Roots by the address of their programs: the main program of each IDL first,
 * then the programs an IDL brings along, where no IDL is about them.
 */
export function byProgram(roots) {
    const found = new Map();
    for (const root of roots) found.set(root.program.publicKey, root);
    for (const root of roots) {
        for (const program of root.additionalPrograms ?? []) {
            if (!found.has(program.publicKey)) found.set(program.publicKey, root);
        }
    }
    return found;
}

/** Codama roots by program address, from IDL JSON texts. */
export function loadRoots(idlTexts) {
    return byProgram(idlTexts.map(text => createFromJson(text).getRoot()));
}

/** `accounts`: address -> { data: base64, owner?: address }, for fetchAccount. */
export function makeFetchAccount(accounts = {}) {
    return async address => {
        const entry = accounts[address];
        if (!entry) return { address, exists: false };
        const data = Uint8Array.from(Buffer.from(typeof entry === 'string' ? entry : entry.data, 'base64'));
        return {
            address,
            data,
            executable: false,
            exists: true,
            lamports: 0n,
            programAddress: (typeof entry === 'object' && entry.owner) || '11111111111111111111111111111111',
            space: BigInt(data.length),
        };
    };
}

/** What Codama displays for each instruction: its display, `null` when it cannot identify it. */
export async function displayInstructions(instructions, roots, fetchAccount) {
    const out = [];
    for (const instruction of instructions) {
        const root = roots.get(instruction.programAddress);
        let display = null;
        let error;
        if (root) {
            try {
                display = await getInstructionDisplay(root, instruction, { fetchAccount });
            } catch (e) {
                error = String(e.message ?? e);
            }
        }
        out.push({ display, program: instruction.programAddress, ...(error ? { error } : {}) });
    }
    return out;
}
