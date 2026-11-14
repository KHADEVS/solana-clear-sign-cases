// Codama's own instruction display for a Solana message: decodes the message
// (legacy or v0), finds each instruction's program in the given Codama IDLs,
// and returns what @codama/dynamic-instructions shows for it.

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
        first = u8();
    }
    const [signers, readonlySigned, readonlyUnsigned] = [first, u8(), u8()];
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

/** Codama roots by program address, from IDL JSON texts. */
export function loadRoots(idlTexts) {
    const roots = new Map();
    for (const text of idlTexts) {
        const root = createFromJson(text).getRoot();
        roots.set(root.program.publicKey, root);
    }
    return roots;
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
