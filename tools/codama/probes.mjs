// Small IDLs written for the purpose, one per behaviour of the display that the
// real programs here do not reach: every type node, every display node, the
// ways an instruction is found, and data that is too short, too long or not
// in its usual form. Each probe records what Codama shows for it, so that
// `cargo test --test probes` can hold the Rust display to the same answers.
//
//   node tools/codama/probes.mjs           writes tests/probes.json
//   node tools/codama/probes.mjs --check   fails if Codama no longer gives what the file holds

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { getInstructionDisplay as displayNext } from 'dynamic-instructions-rc';
import { createFromRoot as createNext } from 'codama-rc';
import { upgradeFromJson } from 'codama-upgrade-rc';
import { getInstructionDisplay } from '@codama/dynamic-instructions';
import { AccountRole } from '@solana/instructions';
import { createFromJson } from 'codama';

import { makeFetchAccount } from './display.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const file = path.resolve(here, '../../tests/probes.json');

const PROGRAM = 'Probe11111111111111111111111111111111111111';
// Addresses that are only names here.
const A = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
const B = 'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB';
const C = 'CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC';
const D = 'DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD';

// --- Nodes -------------------------------------------------------------------

const num = (format, extra = {}) => ({ kind: 'numberTypeNode', format, endian: 'le', ...extra });
const u8 = num('u8');
const u16 = num('u16');
const u32 = num('u32');
const u64 = num('u64');
const key = { kind: 'publicKeyTypeNode' };
const bytes = { kind: 'bytesTypeNode' };
const string = (encoding = 'utf8') => ({ kind: 'stringTypeNode', encoding });
const bool = size => ({ kind: 'booleanTypeNode', size: size ?? u8 });
const fixed = (size, type) => ({ kind: 'fixedSizeTypeNode', size, type });
const prefixed = (prefix, type) => ({ kind: 'sizePrefixTypeNode', prefix, type });
const option = (item, extra = {}) => ({ kind: 'optionTypeNode', item, prefix: u8, fixed: false, ...extra });
const field = (name, type, display) => ({ kind: 'structFieldTypeNode', name, type, docs: [], ...(display ? { display } : {}) });
const struct = (...fields) => ({ kind: 'structTypeNode', fields });
const tuple = (...items) => ({ kind: 'tupleTypeNode', items });
const count = {
    fixed: value => ({ kind: 'fixedCountNode', value }),
    prefixed: prefix => ({ kind: 'prefixedCountNode', prefix }),
    rest: { kind: 'remainderCountNode' },
};
const array = (item, c) => ({ kind: 'arrayTypeNode', item, count: c });
const set = (item, c) => ({ kind: 'setTypeNode', item, count: c });
const map = (k, value, c) => ({ kind: 'mapTypeNode', key: k, value, count: c });
const empty = (name, extra = {}) => ({ kind: 'enumEmptyVariantTypeNode', name, ...extra });
const enumOf = (variants, size) => ({ kind: 'enumTypeNode', variants, size: size ?? u8 });
const link = name => ({ kind: 'definedTypeLinkNode', name });
const number = n => ({ kind: 'numberValueNode', number: n });
const constant = (type, value) => ({ kind: 'constantValueNode', type, value });
const bytesValue = (encoding, data) => ({ kind: 'bytesValueNode', encoding, data });
const injectedValue = (k, fallback) => ({ kind: 'injectedValueNode', key: k, ...(fallback ? { fallback } : {}) });
const amount = (type, extra = {}) => ({ ...type, display: { kind: 'amountNumberDisplayNode', ...extra } });
const fieldDisplay = extra => ({ kind: 'structFieldDisplayNode', ...extra });

const arg = (name, type, extra = {}) => ({ kind: 'instructionArgumentNode', name, type, docs: [], ...extra });
const account = (name, extra = {}) => ({ kind: 'instructionAccountNode', name, isWritable: false, isSigner: false, isOptional: false, docs: [], ...extra });
const tag = value => arg('discriminator', u8, { defaultValue: number(value), defaultValueStrategy: 'omitted', display: fieldDisplay({ skip: 'always' }) });
const byTag = [{ kind: 'fieldDiscriminatorNode', name: 'discriminator', offset: 0 }];

/** An instruction that starts with a one-byte tag, 7 unless told otherwise. */
const ix = (args, extra = {}) => ({
    kind: 'instructionNode',
    name: 'probe',
    accounts: [],
    arguments: [tag(extra.tag ?? 7), ...args],
    discriminators: byTag,
    docs: [],
    optionalAccountStrategy: 'programId',
    ...extra,
    tag: undefined,
});

const program = (instructions, extra = {}) => ({
    kind: 'rootNode',
    standard: 'codama',
    version: '1.0.0',
    program: {
        kind: 'programNode',
        name: 'probe',
        publicKey: PROGRAM,
        version: '0.0.0',
        origin: 'shank',
        accounts: [],
        definedTypes: [],
        errors: [],
        instructions,
        pdas: [],
        ...extra,
    },
    additionalPrograms: [],
});

// --- Probes ------------------------------------------------------------------

const probes = [];
const hex = text => Uint8Array.from(Buffer.from(text.replace(/\s+/g, ''), 'hex'));
/** A number on eight bytes, low byte first, in hex. */
const le = value => value.toString(16).padStart(16, '0').match(/../g).reverse().join('');
/** `data` is written in hex, spaces allowed; the tag 07 comes first unless `raw`. */
function probe(name, idl, data, extra = {}) {
    probes.push({ accounts: [], fetch: {}, name, ...extra, data: hex(extra.raw ? data : `07 ${data}`), idl });
}
/** One argument named `value` of the given type. */
const one = (name, type, data, extra = {}) => probe(name, program([ix([arg('value', type, extra.arg ?? {})], extra.ix ?? {})], extra.program ?? {}), data, extra);

// Numbers.
one('u8', u8, 'ff');
one('u16', u16, '0102');
one('u32', u32, '01020304');
one('u64, above what a double holds', u64, 'ffffffffffffffff');
one('u128', num('u128'), 'ffffffffffffffffffffffffffffffff');
one('i8, negative', num('i8'), '80');
one('i16, negative', num('i16'), 'feff');
one('i32, negative', num('i32'), '00000080');
one('i64, negative', num('i64'), '0000000000000080');
one('i128, negative', num('i128'), 'ffffffffffffffffffffffffffffffff');
one('f32', num('f32'), '0000c03f');
one('f32, not exact in decimal', num('f32'), 'cdcccc3d');
one('f64', num('f64'), '9a9999999999b93f');
one('f64, very large', num('f64'), '7dc39425ad49b254');
one('f64, very small', num('f64'), '3a8c30e28e79453e');
one('f64, not a number', num('f64'), '000000000000f87f');
one('f64, infinite', num('f64'), '000000000000f07f');
one('f64, minus zero', num('f64'), '0000000000000080');
one('u16, big endian', num('u16', { endian: 'be' }), '0100');
one('u32, big endian', num('u32', { endian: 'be' }), '00000102');
one('u64, big endian', num('u64', { endian: 'be' }), '0000000000000102');
one('i16, big endian, negative', num('i16', { endian: 'be' }), 'fffe');
one('short u16, one byte', num('shortU16'), '7f');
one('short u16, two bytes', num('shortU16'), '8001');
one('short u16, three bytes', num('shortU16'), 'ffff03');
one('short u16, a third byte that overflows', num('shortU16'), 'ffff7f');
one('short u16, four bytes', num('shortU16'), 'ffffff01');
one('short u16, padded with a zero byte', num('shortU16'), '8000');
one('a number cut short', u32, '0102');
one('bytes after the last argument', u8, '01 0203');

// Booleans.
one('boolean, true', bool(), '01');
one('boolean, false', bool(), '00');
one('boolean, a byte of 2', bool(), '02');
one('boolean on four bytes', bool(u32), '01000000');
one('boolean on four bytes, 256', bool(u32), '00010000');

// Addresses, bytes and text.
one('address', key, '00'.repeat(31) + '01');
one('address cut short', key, '00'.repeat(31));
one('bytes to the end', bytes, '0102ff');
one('bytes to the end, none left', bytes, '');
one('bytes of a fixed size', fixed(2, bytes), '0102 03');
one('bytes with a length', prefixed(u32, bytes), '02000000 0102 03');
one('bytes with a length longer than the data', prefixed(u32, bytes), '05000000 0102');
one('text to the end', string(), '68c3a9');
one('text of a fixed size, padded with zeros', fixed(8, string()), '6869000000000000');
one('text with zeros in the middle', fixed(4, string()), '68006900');
one('text with a length', prefixed(u32, string()), '02000000 6869 6a');
one('text that is not UTF-8', string(), '68ff69');
one('text with a byte order mark', string(), 'efbbbf6869');
one('text with a quote, a backslash and a line break', string(), '22 5c 0a');
one('text in base16', string('base16'), '0aff');
one('text in base58', string('base58'), '0001ff');
one('text in base64', string('base64'), '0001ff');
one('text shown from its 3rd to its 5th unit', { ...string(), display: { kind: 'stringDisplayNode', sliceStart: 2, sliceEnd: 5 } }, '61626364656667');
one('text shown without its last two units', { ...string(), display: { kind: 'stringDisplayNode', sliceEnd: -2 } }, '61626364656667');
one('text shown from a start after its end', { ...string(), display: { kind: 'stringDisplayNode', sliceStart: 5, sliceEnd: 2 } }, '61626364656667');
one('text sliced in the middle of an emoji', { ...string(), display: { kind: 'stringDisplayNode', sliceStart: 0, sliceEnd: 2 } }, '61f09f9880');

// Options.
one('option, some', option(u16), '01 0500');
one('option, none', option(u16), '00');
one('option, a tag of 2', option(u16), '02 0500');
one('option of a fixed size, none', option(u16, { fixed: true }), '00 0500');
one('option of a fixed size, none, cut short', option(u16, { fixed: true }), '00');
one('option with a four-byte tag', option(u16, { prefix: u32 }), '01000000 0500');
one('option in an option, some of none', option(option(u8)), '01 00');
one('option in an option, some of some', option(option(u8)), '01 01 09');
one('option that takes what is left, some', { kind: 'remainderOptionTypeNode', item: u16 }, '0500');
one('option that takes what is left, none', { kind: 'remainderOptionTypeNode', item: u16 }, '');
one('option that is zero when empty, none', { kind: 'zeroableOptionTypeNode', item: key }, '00'.repeat(32));
one('option that is zero when empty, some', { kind: 'zeroableOptionTypeNode', item: key }, '00'.repeat(31) + '01');
one('option with its own empty value, none', { kind: 'zeroableOptionTypeNode', item: u16, zeroValue: constant(bytes, bytesValue('base16', 'ffff')) }, 'ffff');
one('option with its own empty value, zero', { kind: 'zeroableOptionTypeNode', item: u16, zeroValue: constant(bytes, bytesValue('base16', 'ffff')) }, '0000');
one('option of an amount, some', option(amount(u64, { decimals: number(2), unit: { kind: 'stringValueNode', string: 'EUR' } })), '01 3930000000000000');
one('option of an amount, none', option(amount(u64, { decimals: number(2) })), '00');

// Structs, tuples, lists, sets and maps.
const point = struct(field('x', u8), field('y', u16));
one('struct', point, '01 0200');
one('struct with an address and a big number', struct(field('owner', key), field('amount', u64)), '00'.repeat(31) + '01 ffffffffffffffff');
one('struct in a struct', struct(field('a', point), field('b', bool())), '01 0200 01');
one('struct with the same name twice', struct(field('x', u8), field('y', u8), field('x', u16)), '01 02 0300');
one('struct with a field named like a number', struct(field('b', u8), field('1', u8), field('a', u8), field('0', u8)), '01 02 03 04');
one('tuple', tuple(u8, string('base16')), '01 0aff');
one('list of a fixed length', array(u16, count.fixed(2)), '0100 0200 0300');
one('list of a fixed length, cut short', array(u16, count.fixed(2)), '0100');
one('list with a count', array(u8, count.prefixed(u32)), '03000000 010203');
one('list with a count, nothing left for the count', array(u8, count.prefixed(u32)), '');
one('list with a count larger than the data', array(u8, count.prefixed(u32)), '03000000 0102');
one('list to the end', array(u16, count.rest), '0100 0200');
one('list to the end, with a byte too many', array(u16, count.rest), '0100 02');
one('list of nothing, a fixed number of times', array(tuple(), count.fixed(3)), '');
one('list of structs', array(point, count.fixed(2)), '01 0200 03 0400');
one('list of lists', array(array(u8, count.fixed(2)), count.fixed(2)), '01020304');
one('list of addresses, two', array(key, count.fixed(2)), '00'.repeat(31) + '01' + '00'.repeat(31) + '02');
one('list of addresses, one', array(key, count.fixed(1)), '00'.repeat(31) + '01');
one('list of addresses, none', array(key, count.prefixed(u8)), '00');
one('option of a list of addresses', option(array(key, count.fixed(2))), '01' + '00'.repeat(31) + '01' + '00'.repeat(31) + '02');
one('list of amounts', array(amount(u64, { decimals: number(6) }), count.fixed(2)), '40420f0000000000 80841e0000000000');
one('list of options', array(option(u8), count.fixed(3)), '0105 00 0107');
one('list of booleans', array(bool(), count.fixed(2)), '0100');
one('list of text', array(prefixed(u8, string()), count.fixed(2)), '0161 0162');
one('set', set(u8, count.prefixed(u8)), '03 020102');
one('map from numbers to numbers', map(u8, u16, count.prefixed(u8)), '03 09 0100 02 0200 09 0300');
one('map from text to text', map(prefixed(u8, string()), prefixed(u8, string()), count.fixed(2)), '0162 0131 0161 0132');
one('map from big numbers', map(u64, u8, count.fixed(2)), '0500000000000000 01 0100000000000000 02');
one('map from addresses', map(key, u8, count.fixed(1)), '00'.repeat(31) + '01 07');
one('map from booleans', map(bool(), u8, count.fixed(2)), '01 05 00 06');
one('map from structs', map(point, u8, count.fixed(1)), '01 0200 09');

// Enums.
const colour = enumOf([empty('red'), empty('darkGreen'), empty('BLUE_ISH')]);
one('enum, first variant', colour, '00');
one('enum, a name in two words', colour, '01');
one('enum, a name in capitals', colour, '02');
one('enum, a tag with no variant', colour, '03');
one('enum on four bytes', enumOf([empty('red'), empty('green')], u32), '01000000');
one('enum with a label', enumOf([empty('red'), empty('green', { display: { kind: 'enumVariantDisplayNode', label: 'Go ahead' } })]), '01');
one('enum with tags of its own', enumOf([empty('low', { discriminator: 10 }), empty('high', { discriminator: 20 })]), '14');
one('enum with tags of its own, the position instead', enumOf([empty('low', { discriminator: 10 }), empty('high', { discriminator: 20 })]), '01');
const shape = enumOf([
    empty('nothing'),
    { kind: 'enumStructVariantTypeNode', name: 'circle', struct: struct(field('radius', u16)) },
    { kind: 'enumTupleVariantTypeNode', name: 'pair', tuple: tuple(u8, u8) },
    { kind: 'enumStructVariantTypeNode', name: 'named', struct: struct(field('__kind', u8), field('fields', u8)), display: { kind: 'enumVariantDisplayNode', label: 'With a label' } },
]);
one('enum, a variant with nothing', shape, '00');
one('enum, a variant with a struct', shape, '01 0500');
one('enum, a variant with a tuple', shape, '02 0102');
one('enum, a variant whose fields take reserved names', shape, '03 0102');
one('enum, a variant cut short', shape, '01 05');
one('list of enums', array(shape, count.fixed(3)), '00 01 0500 02 0102');
one('option of an enum', option(colour), '01 01');
one('struct with an enum', struct(field('colour', colour), field('shape', shape)), '01 01 0500');

// Links.
const types = { definedTypes: [
    { kind: 'definedTypeNode', name: 'point', type: point, docs: [] },
    { kind: 'definedTypeNode', name: 'colour', type: colour, docs: [] },
    { kind: 'definedTypeNode', name: 'shape', type: shape, docs: [] },
    { kind: 'definedTypeNode', name: 'price', type: amount(u64, { decimals: number(2), unit: { kind: 'stringValueNode', string: 'USD' } }), docs: [] },
    { kind: 'definedTypeNode', name: 'alias', type: link('price'), docs: [] },
    { kind: 'definedTypeNode', name: 'loop', type: link('loop'), docs: [] },
    { kind: 'definedTypeNode', name: 'tree', type: struct(field('value', u8), field('next', option(link('tree')))), docs: [] },
] };
one('link to a struct', link('point'), '01 0200', { program: types });
one('link to an enum', link('colour'), '01', { program: types });
one('link to an amount', link('price'), '3930000000000000', { program: types });
one('link to a link to an amount', link('alias'), '3930000000000000', { program: types });
one('option of a link to an amount', option(link('price')), '01 3930000000000000', { program: types });
one('list of links to an amount', array(link('price'), count.fixed(2)), '6400000000000000 c800000000000000', { program: types });
one('link to a type that is not there', link('missing'), '01', { program: types });
one('link to itself', link('loop'), '01', { program: types });
one('link to a type that holds itself', link('tree'), '01 01 02 00', { program: types });

// Hidden constants, sentinels and offsets.
const magic = constant(bytes, bytesValue('base16', 'cafe'));
one('hidden prefix', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [magic] }, 'cafe 05');
one('hidden prefix, not the expected bytes', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [magic] }, 'beef 05');
one('hidden suffix', { kind: 'hiddenSuffixTypeNode', type: u8, suffix: [magic, constant(u8, number(1))] }, '05 cafe 01');
one('hidden suffix, missing', { kind: 'hiddenSuffixTypeNode', type: u8, suffix: [magic] }, '05');
const padded = (kind, offset, strategy = 'padded') => constant({ kind, offset, strategy, type: u8 }, number(1));
one('hidden prefix after padding', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('preOffsetTypeNode', 2)] }, '0000 01 05');
one('hidden prefix after padding that is not zeros', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('preOffsetTypeNode', 2)] }, 'aa00 01 05');
one('hidden prefix before padding', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('postOffsetTypeNode', 2)] }, '01 0000 05');
one('hidden prefix before padding that is not zeros', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('postOffsetTypeNode', 2)] }, '01 00aa 05');
one('hidden prefix after a gap that is not padding', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('preOffsetTypeNode', 2, 'relative')] }, '0000 01 05');
one('hidden prefix with a gap of nothing', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('preOffsetTypeNode', 0, 'relative')] }, '01 05');
one('hidden prefix at a fixed place', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('preOffsetTypeNode', 0, 'absolute')] }, '01 05');
one('hidden prefix followed by a step back', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [padded('postOffsetTypeNode', -1, 'relative')] }, '01 05');
one('hidden prefix that is a fixed size of text', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(fixed(4, string()), { kind: 'stringValueNode', string: 'ab' })] }, '61620000 05');
one('hidden prefix that is text with a length', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(prefixed(u16, string()), { kind: 'stringValueNode', string: 'ab' })] }, '0200 6162 05');
one('hidden prefix that is an address', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(key, { kind: 'publicKeyValueNode', publicKey: '11111111111111111111111111111112' })] }, '00'.repeat(31) + '01 05');
one('hidden prefix that is a boolean', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(bool(), { kind: 'booleanValueNode', boolean: true })] }, '01 05');
one('hidden prefix that is a negative number', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(num('i16'), number(-2))] }, 'feff 05');
one('hidden prefix that is a number too large for its type', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(u8, number(256))] }, '00 05');
one('hidden prefix that is a decimal number', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(num('f32'), number(1.5))] }, '0000c03f 05');
one('hidden prefix that is a short u16', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(num('shortU16'), number(128))] }, '8001 05');
one('hidden prefix that is a variant of an enum defined nowhere', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(enumOf([empty('red'), empty('green')]), { kind: 'enumValueNode', enum: link('colour'), variant: 'green' })] }, '01 05');
one('hidden prefix that is a number given as text', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(string(), { kind: 'stringValueNode', string: 'hi' })] }, '6869 05');
one('text up to a sentinel', { kind: 'sentinelTypeNode', type: string(), sentinel: constant(bytes, bytesValue('base16', 'ff')) }, '6869 ff 6a');
one('text with no sentinel', { kind: 'sentinelTypeNode', type: string(), sentinel: constant(bytes, bytesValue('base16', 'ff')) }, '6869');
one('list up to a sentinel', { kind: 'sentinelTypeNode', type: array(u8, count.rest), sentinel: constant(bytes, bytesValue('base16', 'ffff')) }, '0102 ffff 03');
const after = (type, offset, strategy) => ({ kind: 'postOffsetTypeNode', type, offset, strategy });
const before = (type, offset, strategy) => ({ kind: 'preOffsetTypeNode', type, offset, strategy });
const two = (name, first, data) => probe(name, program([ix([arg('first', first), arg('second', u8)])]), data);
two('a gap after a value', after(u8, 2, 'relative'), '01 aaaa 02');
two('a gap after a value, padded', after(u8, 2, 'padded'), '01 aaaa 02');
two('a step back after a value', after(u16, -1, 'relative'), '0102');
two('the next value at a fixed place', after(u8, 3, 'absolute'), '01 aa 02');
two('the next value counted from the end', after(u8, -1, 'absolute'), '01 aaaa 02');
two('the next value where this one started', after(u16, 0, 'preOffset'), '0102');
two('a gap before a value', before(u8, 2, 'relative'), 'aaaa 01 02');
two('a gap before a value, padded', before(u8, 2, 'padded'), 'aaaa 01 02');
two('a value at a fixed place', before(u8, 0, 'absolute'), '');
two('a value counted from the end', before(u8, -1, 'absolute'), 'aa 02');
two('a gap that leaves the data', before(u8, 9, 'relative'), '01 02');
two('a value after an option with a tag of 2', option(u16), '02 05 00');
two('a value after an option of a fixed size with a tag of 2', option(u16, { fixed: true }), '02 0500 07');
one('hidden prefix that is a variant of a linked enum', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(link('colour'), { kind: 'enumValueNode', enum: link('colour'), variant: 'darkGreen' })] }, '01 05', { program: types });
one('hidden prefix that is a tuple', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(tuple(u8, u8), { kind: 'tupleValueNode', items: [number(1), number(2)] })] }, '0102 05');
one('hidden prefix that is a present option', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(option(u8), { kind: 'someValueNode', value: number(9) })] }, '0109 05');
one('hidden prefix that is an absent option', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(option(u8), { kind: 'noneValueNode' })] }, '00 05');
one('hidden prefix that is an absent option of a fixed size', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(option(u16, { fixed: true }), { kind: 'noneValueNode' })] }, '00 0000 05');
one('hidden prefix that is a struct', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(point, { kind: 'structValueNode', fields: [{ kind: 'structFieldValueNode', name: 'y', value: number(2) }, { kind: 'structFieldValueNode', name: 'x', value: number(1) }] })] }, '01 0200 05');
one('hidden prefix that is a list with a count', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(array(u8, count.prefixed(u16)), { kind: 'arrayValueNode', items: [number(7), number(8)] })] }, '0200 0708 05');
one('hidden prefix that is a list of a fixed length', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(array(u8, count.fixed(2)), { kind: 'arrayValueNode', items: [number(7), number(8)] })] }, '0708 05');
one('hidden prefix that is a variant with a struct', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(shape, { kind: 'enumValueNode', enum: link('shape'), variant: 'circle', value: { kind: 'structValueNode', fields: [{ kind: 'structFieldValueNode', name: 'radius', value: number(5) }] } })] }, '01 0500 05', { program: types });
one('hidden prefix that is a variant with a tuple', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(shape, { kind: 'enumValueNode', enum: link('shape'), variant: 'pair', value: { kind: 'tupleValueNode', items: [number(1), number(2)] } })] }, '02 0102 05', { program: types });
one('hidden prefix that is a variant the enum does not have', { kind: 'hiddenPrefixTypeNode', type: u8, prefix: [constant(colour, { kind: 'enumValueNode', enum: link('colour'), variant: 'purple' })] }, '01 05', { program: types });

// Display nodes on numbers.
one('amount, no decimals given', amount(u64), '3930000000000000');
one('amount, two decimals', amount(u64, { decimals: number(2) }), '3930000000000000');
one('amount, more decimals than digits', amount(u64, { decimals: number(9) }), '0100000000000000');
one('amount, nothing after the point', amount(u64, { decimals: number(3) }), 'e803000000000000');
one('amount of zero', amount(u64, { decimals: number(6) }), '0000000000000000');
one('amount, the largest u64 with nine decimals', amount(u64, { decimals: number(9) }), 'ffffffffffffffff');
one('amount, negative', amount(num('i64'), { decimals: number(2) }), 'c7cfffffffffffff');
one('amount on a small number', amount(u16, { decimals: number(2) }), '3930');
one('amount on a decimal number', amount(num('f64'), { decimals: number(2) }), '9a9999999999b93f');
one('amount with a unit', amount(u64, { decimals: number(9), unit: { kind: 'stringValueNode', string: 'SOL' } }), '00ca9a3b00000000');
one('amount with an empty unit', amount(u64, { decimals: number(9), unit: { kind: 'stringValueNode', string: '' } }), '00ca9a3b00000000');
one('amount with decimals that are not whole', amount(u64, { decimals: number(1.5) }), '3930000000000000');
one('amount with negative decimals', amount(u64, { decimals: number(-1) }), '3930000000000000');
one('amount whose decimals nothing provides', amount(u64, { decimals: injectedValue('decimals') }), '3930000000000000');
one('amount whose decimals fall back to a number', amount(u64, { decimals: injectedValue('decimals', number(3)) }), '3930000000000000');
one('date', { ...num('i64'), display: { kind: 'dateTimeNumberDisplayNode' } }, '00f1536500000000');
one('date before 1970', { ...num('i64'), display: { kind: 'dateTimeNumberDisplayNode' } }, 'ffffffffffffffff');
one('date in milliseconds', { ...u64, display: { kind: 'dateTimeNumberDisplayNode', ticksPerSecond: 1000 } }, '8b1ce5d08b010000');
one('date too far to be one', { ...u64, display: { kind: 'dateTimeNumberDisplayNode' } }, 'ffffffffffffffff');
one('date in the year 10000', { ...u64, display: { kind: 'dateTimeNumberDisplayNode' } }, le(253402300800n));
one('date on the last day JavaScript knows', { ...u64, display: { kind: 'dateTimeNumberDisplayNode' } }, le(8640000000000n));
one('date a second after the last one JavaScript knows', { ...u64, display: { kind: 'dateTimeNumberDisplayNode' } }, le(8640000000001n));
one('date with a part of a second', { ...u64, display: { kind: 'dateTimeNumberDisplayNode', ticksPerSecond: 3 } }, le(5n));
one('date in the year 0', { ...num('i64'), display: { kind: 'dateTimeNumberDisplayNode' } }, le(BigInt.asUintN(64, -62167219200n)));
one('date before the year 0', { ...num('i64'), display: { kind: 'dateTimeNumberDisplayNode' } }, le(BigInt.asUintN(64, -62198755200n)));
one('duration', { ...u64, display: { kind: 'durationNumberDisplayNode' } }, '950e000000000000');
one('duration over a day', { ...u64, display: { kind: 'durationNumberDisplayNode' } }, le(86400n));
one('duration over a hundred hours', { ...u64, display: { kind: 'durationNumberDisplayNode' } }, le(360000n + 61n));
one('duration longer than a double counts exactly', { ...u64, display: { kind: 'durationNumberDisplayNode' } }, le(2n ** 64n - 1n));
one('duration in milliseconds', { ...u32, display: { kind: 'durationNumberDisplayNode', ticksPerSecond: 1000 } }, 'dc050000');
one('duration, negative', { ...num('i32'), display: { kind: 'durationNumberDisplayNode' } }, 'ffffffff');
one('duration with zero ticks per second', { ...u32, display: { kind: 'durationNumberDisplayNode', ticksPerSecond: 0 } }, 'dc050000');
one('a display node of an unknown kind', { ...u32, display: { kind: 'somethingElseDisplayNode' } }, 'dc050000');
one('older amount type', { kind: 'amountTypeNode', decimals: 2, unit: 'USD', number: u64 }, '3930000000000000');
one('older SOL amount type', { kind: 'solAmountTypeNode', number: u64 }, '00ca9a3b00000000');
one('older date type', { kind: 'dateTimeTypeNode', number: num('i64') }, '00f1536500000000');

// Labels, skipped fields and flattened structs.
const labelled = struct(field('first', u8, fieldDisplay({ label: 'The First' })), field('hidden', u8, fieldDisplay({ skip: 'always' })), field('lastOne', u8));
one('a label on an argument', u8, '05', { arg: { display: fieldDisplay({ label: 'Chosen Label' }) } });
one('an argument that is never shown', u8, '05', { arg: { display: fieldDisplay({ skip: 'always' }) } });
one('an argument marked as shown', u8, '05', { arg: { display: fieldDisplay({ skip: 'never' }) } });
one('a struct shown field by field', labelled, '010203', { arg: { display: fieldDisplay({ flatten: true }) } });
one('a struct shown field by field, with a prefix', labelled, '010203', { arg: { display: fieldDisplay({ flatten: true, flattenPrefix: 'Config: ' }) } });
one('a struct not flattened keeps its field labels out', labelled, '010203');
one('a number asked to be flattened', u8, '05', { arg: { display: fieldDisplay({ flatten: true }) } });
one('an option of a struct shown field by field, some', option(labelled), '01 010203', { arg: { display: fieldDisplay({ flatten: true }) } });
one('an option of a struct shown field by field, none', option(labelled), '00', { arg: { display: fieldDisplay({ flatten: true }) } });
one('a linked struct shown field by field', link('point'), '01 0200', { arg: { display: fieldDisplay({ flatten: true }) }, program: types });
one('a struct in a struct shown field by field', struct(field('inner', point, fieldDisplay({ flatten: true })), field('z', u8)), '01 0200 03', { arg: { display: fieldDisplay({ flatten: true }) } });
one('a flattened struct with a list of addresses', struct(field('keys', array(key, count.fixed(2)))), '00'.repeat(31) + '01' + '00'.repeat(31) + '02', { arg: { display: fieldDisplay({ flatten: true }) } });
probe('names turned into labels', program([ix([arg('simple', u8), arg('twoWords', u8), arg('minSOLAmount', u8), arg('v2Pool', u8), arg('a', u8), arg('x1y2', u8)])]), '01 02 03 04 05 06');
probe('names that are not in camel case', program([ix([arg('with_underscore', u8), arg('HTTPServer', u8), arg('MAX_SIZE', u8), arg('kebab-case', u8)])]), '01 02 03 04');
probe('a struct with names that are not in camel case', program([ix([arg('value', struct(field('with_underscore', u8), field('Capital', u8)))])]), '01 02');

// Sizes that do not fit what they hold, and nodes with a part left out.
one('a number in fewer bytes than it takes', fixed(2, u32), '0102 03');
one('a number in more bytes than it takes', fixed(4, u16), '01020304 05');
one('a length around a number, longer than the number', prefixed(u8, u16), '03 0102 03 04');
one('a length around a number, shorter than the number', prefixed(u8, u16), '01 05 06');
one('a length of zero around text', prefixed(u8, string()), '00 61');
one('a list with a short u16 count', array(u8, count.prefixed(num('shortU16'))), '8001' + '07'.repeat(128));
one('a map to the end', map(u8, u8, count.rest), '01 02 03 04');
one('a set of addresses', set(key, count.fixed(2)), '00'.repeat(31) + '01' + '00'.repeat(31) + '02');
one('a struct with a list of addresses, not flattened', struct(field('keys', array(key, count.fixed(1)))), '00'.repeat(31) + '01');
one('boolean with no size given', { kind: 'booleanTypeNode' }, '01');
one('enum with no size given', { kind: 'enumTypeNode', variants: [empty('red'), empty('green')] }, '01');
one('option with no tag type given', { kind: 'optionTypeNode', item: u8 }, '01 05');
one('a number with no byte order given', { kind: 'numberTypeNode', format: 'u16' }, '0102');
one('text with no encoding given', { kind: 'stringTypeNode' }, '6869');
one('a list with no count given', { kind: 'arrayTypeNode', item: u8 }, '0102');
one('a type of an unknown kind', { kind: 'somethingElseTypeNode' }, '01');
one('a number of an unknown format', num('u24'), '010203');
one('a part missing in a type the data does not reach', option({ kind: 'booleanTypeNode' }), '00');
one('a link to nothing in a type the data does not reach', option(link('missing')), '00');

// Sentences.
const sentence = (name, text, args, data, extra = {}) =>
    probe(name, program([ix(args, { accounts: extra.accounts ?? [], display: { kind: 'instructionDisplayNode', intent: 'Do it', interpolatedIntent: text }, provides: extra.provides })], extra.program ?? {}), data, extra);
sentence('a sentence with an argument', 'Send ${data.value} now', [arg('value', u16)], '0500');
sentence('a sentence with an account', 'Pay ${accounts.payee}', [], '', { accounts: [account('payee')], accountsGiven: [[A, false]] });
sentence('a sentence with an account that is not given', 'Pay ${accounts.payee}', [], '', { accounts: [account('payee')] });
sentence('a sentence with an argument that is not there', 'Send ${data.missing} now', [arg('value', u16)], '0500');
sentence('a sentence with spaces in a placeholder', 'Send ${ data.value } now', [arg('value', u16)], '0500');
sentence('a sentence with something else in braces', 'Send ${value} and ${data.value.x} and $ {data.value}', [arg('value', u16)], '0500');
sentence('a sentence with an unclosed placeholder', 'Send ${data.value', [arg('value', u16)], '0500');
sentence('a sentence with the same argument twice', '${data.value} is ${data.value}', [arg('value', u16)], '0500');
sentence('a sentence with an enum and an option', '${data.colour} then ${data.maybe}', [arg('colour', colour), arg('maybe', option(u8))], '01 00');
sentence('a sentence with a struct', 'At ${data.where}', [arg('where', point)], '01 0200');
sentence('a sentence with a skipped argument', 'Tag ${data.discriminator}', [], '');
sentence('a sentence with an amount', 'Send ${data.amount}', [arg('amount', amount(u64, { decimals: number(2), unit: { kind: 'stringValueNode', string: 'USD' } }))], '3930000000000000');
sentence('a sentence with an amount that cannot be scaled', 'Send ${data.amount}', [arg('amount', amount(u64, { decimals: injectedValue('decimals') }))], '3930000000000000');
sentence('a sentence with no placeholder', 'Just do it', [], '');
sentence('an empty sentence', '', [arg('value', u8)], '05');
sentence('a sentence with an account that is never shown', 'Pay ${accounts.payee}', [], '', { accounts: [account('payee', { display: { kind: 'instructionAccountDisplayNode', skip: 'always' } })], accountsGiven: [[A, false]] });
sentence('a sentence with the label of an account', 'Pay ${accounts.Payee} or ${accounts.to}', [], '', { accounts: [account('payee', { display: { kind: 'instructionAccountDisplayNode', label: 'to' } })], accountsGiven: [[A, false]] });
sentence('a sentence with a list of addresses', 'To ${data.keys}', [arg('keys', array(key, count.fixed(2)))], '00'.repeat(31) + '01' + '00'.repeat(31) + '02');
sentence('a sentence with text that holds a placeholder', 'Memo ${data.memo} for ${data.value}', [arg('value', u8), arg('memo', string())], '05 247b646174612e76616c75657d');
probe('a sentence and no intent', program([ix([arg('value', u8)], { name: 'sendIt', display: { kind: 'instructionDisplayNode', interpolatedIntent: 'Send ${data.value}' } })]), '05');
probe('an intent and no sentence', program([ix([], { display: { kind: 'instructionDisplayNode', intent: 'Do it' } })]), '');
probe('no display at all, a name in two words', program([ix([], { name: 'closeTheAccount' })]), '');

// Values taken from elsewhere.
const scaled = amount(u64, { decimals: injectedValue('decimals'), unit: injectedValue('unit') });
const provide = (name, node) => ({ kind: 'providedNode', name, node });
const mintAccount = { kind: 'accountNode', name: 'mint', docs: [], data: struct(field('supply', u64), field('decimals', u8), field('symbol', fixed(4, string()))) };
const mintData = Buffer.from('00e1f50500000000 06 55534443'.replace(/\s+/g, ''), 'hex').toString('base64');
const withMint = { accounts: [account('mint', { accountLink: { kind: 'accountLinkNode', name: 'mint' } })], program: { accounts: [mintAccount] } };
const fromMint = [provide('decimals', { kind: 'accountFieldValueNode', account: 'mint', path: 'decimals' }), provide('unit', { kind: 'accountFieldValueNode', account: 'mint', path: 'symbol' })];
sentence('decimals from another argument', 'Send ${data.amount}', [arg('amount', scaled), arg('decimals', u8, { display: fieldDisplay({ skip: 'whenInjected' }) })], '40420f0000000000 06',
    { provides: [provide('decimals', { kind: 'argumentValueNode', name: 'decimals' })] });
sentence('decimals from another argument, which stays shown', 'Send ${data.amount}', [arg('amount', scaled), arg('decimals', u8)], '40420f0000000000 06',
    { provides: [provide('decimals', { kind: 'argumentValueNode', name: 'decimals' })] });
sentence('decimals from an argument that is not a number', 'Send ${data.amount}', [arg('amount', scaled), arg('decimals', bool())], '40420f0000000000 01',
    { provides: [provide('decimals', { kind: 'argumentValueNode', name: 'decimals' })] });
sentence('decimals and unit from an account', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], fetch: { [A]: mintData }, provides: fromMint });
sentence('decimals from an account, hidden once used', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { accounts: [account('mint', { accountLink: { kind: 'accountLinkNode', name: 'mint' }, display: { kind: 'instructionAccountDisplayNode', skip: 'whenInjected' } })], accountsGiven: [[A, false]], fetch: { [A]: mintData }, program: { accounts: [mintAccount] }, provides: fromMint });
sentence('decimals from an account that another program owns', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], fetch: { [A]: { data: mintData, owner: B } }, provides: fromMint });
sentence('decimals from an account with bytes after its layout', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], fetch: { [A]: Buffer.from('00e1f50500000000 06 55534443 ffffffff'.replace(/\s+/g, ''), 'hex').toString('base64') }, provides: fromMint });
sentence('decimals from an account that does not exist', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], provides: fromMint });
sentence('decimals from an account that is too short', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], fetch: { [A]: Buffer.from('00e1f505', 'hex').toString('base64') }, provides: fromMint });
sentence('decimals from an account with no layout', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { accounts: [account('mint')], accountsGiven: [[A, false]], fetch: { [A]: mintData }, program: { accounts: [mintAccount] }, provides: fromMint });
sentence('decimals from a field the account does not have', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], fetch: { [A]: mintData }, provides: [provide('decimals', { kind: 'accountFieldValueNode', account: 'mint', path: 'missing' })] });
sentence('a unit that is an account address', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { ...withMint, accountsGiven: [[A, false]], provides: [provide('decimals', number(6)), provide('unit', { kind: 'accountValueNode', name: 'mint' })] });
sentence('the same key provided twice', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { provides: [provide('decimals', number(1)), provide('decimals', number(2))] });
sentence('a provided value that points to itself', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { provides: [provide('decimals', injectedValue('decimals'))] });
sentence('a provided value that points to another', 'Send ${data.amount}', [arg('amount', scaled)], '40420f0000000000',
    { provides: [provide('decimals', injectedValue('other')), provide('other', number(4))] });

// Accounts.
const accounts = (name, nodes, given, extra = {}) => probe(name, program([ix([], { accounts: nodes, ...extra })]), '', { accountsGiven: given });
accounts('accounts with and without labels', [account('source', { display: { kind: 'instructionAccountDisplayNode', label: 'From' } }), account('tokenProgram')], [[A, false], [B, false]]);
accounts('an account that is never shown', [account('source', { display: { kind: 'instructionAccountDisplayNode', skip: 'always' } }), account('other')], [[A, false], [B, false]]);
accounts('fewer accounts than named', [account('source'), account('destination'), account('authority')], [[A, false]]);
accounts('more accounts than named', [account('source')], [[A, false], [B, true], [C, false]]);
accounts('the same account twice', [account('source'), account('destination')], [[A, false], [A, false]]);
accounts('an optional account left out, the program in its place', [account('source'), account('maybe', { isOptional: true }), account('last')], [[A, false], [PROGRAM, false], [B, false]]);
accounts('an optional account left out, nothing in its place', [account('source'), account('maybe', { isOptional: true }), account('last')], [[A, false], [B, false]], { optionalAccountStrategy: 'omitted' });
const remaining = (extra = {}) => ({ kind: 'instructionRemainingAccountsNode', value: { kind: 'argumentValueNode', name: 'signers' }, isOptional: true, isSigner: true, ...extra });
accounts('remaining accounts, several', [account('source')], [[A, false], [B, true], [C, true]], { remainingAccounts: [remaining()] });
accounts('remaining accounts, one', [account('source')], [[A, false], [B, true]], { remainingAccounts: [remaining()] });
accounts('remaining accounts, none', [account('source')], [[A, false]], { remainingAccounts: [remaining()] });
accounts('remaining accounts with a label', [account('source')], [[A, false], [B, true]], { remainingAccounts: [remaining({ display: { kind: 'instructionAccountDisplayNode', label: 'Co-signer' } })] });
accounts('remaining accounts that are never shown', [account('source')], [[A, false], [B, true]], { remainingAccounts: [remaining({ display: { kind: 'instructionAccountDisplayNode', skip: 'always' } })] });
accounts('remaining accounts that are not signers, in a single group of signers', [account('source')], [[A, false], [B, false], [C, true]], { remainingAccounts: [remaining()] });
accounts('remaining accounts in two groups', [], [[A, true], [B, true], [C, false], [D, false]],
    { remainingAccounts: [remaining(), remaining({ isSigner: false, value: { kind: 'argumentValueNode', name: 'readers' } })] });
accounts('remaining accounts in two groups, the first empty', [], [[C, false], [D, false]],
    { remainingAccounts: [remaining(), remaining({ isSigner: false, value: { kind: 'argumentValueNode', name: 'readers' } })] });
accounts('remaining accounts in two groups, a signer after the others', [], [[A, true], [C, false], [B, true]],
    { remainingAccounts: [remaining(), remaining({ isSigner: false, value: { kind: 'argumentValueNode', name: 'readers' } })] });
accounts('remaining accounts, signers or not', [], [[A, true], [C, false]], { remainingAccounts: [remaining({ isSigner: 'either' }), remaining({ isSigner: false, value: { kind: 'argumentValueNode', name: 'readers' } })] });
accounts('remaining accounts found by seeds', [], [[A, false], [B, false]], { remainingAccounts: [{ kind: 'instructionRemainingAccountsNode', value: { kind: 'resolverValueNode', name: 'resolveThem' }, isOptional: true, isSigner: false }] });

// Finding the instruction.
const named = (name, extra) => ({ ...ix([], extra), name });
const find = (name, instructions, data, extra = {}) => probe(name, program(instructions, extra.program ?? {}), data, { raw: true, ...extra });
find('no instruction with this tag', [named('seven', { tag: 7 })], '08');
find('no data at all', [named('seven', { tag: 7 })], '');
find('two instructions with the same tag: the first', [named('first', { tag: 7 }), named('second', { tag: 7 })], '07');
find('a tag given as constant bytes', [{ ...named('constant'), arguments: [], discriminators: [{ kind: 'constantDiscriminatorNode', constant: constant(bytes, bytesValue('base16', 'cafe')), offset: 0 }] }], 'cafe');
find('a tag at an offset', [{ ...named('offset'), arguments: [arg('first', u8), arg('second', u8)], discriminators: [{ kind: 'constantDiscriminatorNode', constant: constant(u8, number(9)), offset: 1 }] }], '0509');
find('a tag and a length, both needed', [{ ...named('sized'), discriminators: [...byTag, { kind: 'sizeDiscriminatorNode', size: 2 }], arguments: [tag(7), arg('value', u8)] }], '0701');
find('a tag and a length, the length is wrong', [{ ...named('sized'), discriminators: [...byTag, { kind: 'sizeDiscriminatorNode', size: 2 }], arguments: [tag(7), arg('value', u8)] }], '070102');
find('found by its length alone', [{ ...named('sized'), discriminators: [{ kind: 'sizeDiscriminatorNode', size: 3 }], arguments: [arg('value', u8)] }], '010203');
find('one instruction, no way to tell it', [{ ...named('only'), discriminators: [], arguments: [arg('value', u8)] }], '05');
find('two instructions, no way to tell them', [{ ...named('one'), discriminators: [], arguments: [arg('value', u8)] }, { ...named('other'), discriminators: [], arguments: [arg('value', u8)] }], '05');
find('a tag whose argument has no value', [{ ...named('broken'), arguments: [arg('discriminator', u8)], discriminators: byTag }], '07');
find('a tag whose argument is missing', [{ ...named('broken'), arguments: [], discriminators: byTag }], '07');
find('a tag in an eight-byte hash', [{ ...named('anchor'), arguments: [arg('discriminator', fixed(8, bytes), { defaultValue: bytesValue('base16', '0102030405060708'), defaultValueStrategy: 'omitted', display: fieldDisplay({ skip: 'always' }) }), arg('value', u8)], discriminators: byTag }], '0102030405060708 09');
find('a tag on two bytes, big endian', [{ ...named('wide'), arguments: [arg('discriminator', num('u16', { endian: 'be' }), { defaultValue: number(258), defaultValueStrategy: 'omitted' })], discriminators: byTag }], '0102');
find('an instruction of another program of the same IDL', [named('main', { tag: 7 })], '07',
    { program: {}, programAddress: A, more: [{ kind: 'programNode', name: 'second', publicKey: A, version: '0.0.0', origin: 'shank', accounts: [], definedTypes: [], errors: [], pdas: [], instructions: [named('fromTheSecond', { tag: 7 })] }] });
find('a link to a type of another program', [{ ...named('main'), arguments: [tag(7), arg('value', { kind: 'definedTypeLinkNode', name: 'far', program: { kind: 'programLinkNode', name: 'second' } })] }], '07 0500',
    { more: [{ kind: 'programNode', name: 'second', publicKey: A, version: '0.0.0', origin: 'shank', accounts: [], definedTypes: [{ kind: 'definedTypeNode', name: 'far', type: amount(u16, { decimals: number(1) }), docs: [] }], errors: [], pdas: [], instructions: [] }] });

// --- Codama's answers ----------------------------------------------------------

/**
 * Text that is not Unicode any more (half of a surrogate pair, left by a slice
 * through a character) cannot be stored as JSON text everywhere: it is written
 * with U+FFFD, and the answer says so.
 */
function wellFormed(display) {
    let broken = false;
    const fix = text => (typeof text === 'string' && !text.isWellFormed() ? ((broken = true), text.toWellFormed()) : text);
    const fixed = display && {
        ...display,
        fields: display.fields.map(f => ({ ...f, label: fix(f.label), value: fix(f.value) })),
        intent: fix(display.intent),
        interpolatedIntent: fix(display.interpolatedIntent),
    };
    return broken ? { display: fixed, halfSurrogate: true } : { display };
}

async function shown(render, root, instruction, fetchAccount) {
    try {
        return wellFormed(await render(root, instruction, { fetchAccount }));
    } catch (e) {
        return { display: null, error: String(e.message ?? e).split('\n')[0].slice(0, 200) };
    }
}

const out = [];
for (const p of probes) {
    const idl = structuredClone(p.idl);
    if (p.more) idl.additionalPrograms = p.more;
    const text = JSON.stringify(idl);
    const instruction = {
        accounts: (p.accountsGiven ?? []).map(([address, signer]) => ({ address, role: signer ? AccountRole.READONLY_SIGNER : AccountRole.READONLY })),
        data: p.data,
        programAddress: p.programAddress ?? PROGRAM,
    };
    const fetchAccount = makeFetchAccount(p.fetch);
    let next;
    try {
        next = await shown(displayNext, createNext(upgradeFromJson(text)).getRoot(), instruction, fetchAccount);
    } catch (e) {
        next = { display: null, error: `the IDL does not upgrade: ${String(e.message ?? e).split('\n')[0].slice(0, 160)}` };
    }
    out.push({
        name: p.name,
        idl: JSON.parse(text),
        program: instruction.programAddress,
        accounts: p.accountsGiven ?? [],
        data: Buffer.from(p.data).toString('hex'),
        fetch: p.fetch,
        stable: await shown(getInstructionDisplay, createFromJson(text).getRoot(), instruction, fetchAccount),
        next,
    });
}

const names = new Set(out.map(p => p.name));
if (names.size !== out.length) throw new Error('two probes have the same name');
// One probe a line.
const text = `[\n${out.map(p => JSON.stringify(p)).join(',\n')}\n]\n`;
if (process.argv.includes('--check')) {
    if (fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n') !== text) {
        console.error('tests/probes.json is not what Codama gives today: run node tools/codama/probes.mjs and look at the diff');
        process.exit(1);
    }
    console.log(`${out.length} probes, all as recorded`);
} else {
    fs.writeFileSync(file, text);
    const count = key => out.filter(p => p[key].display).length;
    console.log(`${out.length} probes written: the stable release shows ${count('stable')}, the release candidate ${count('next')}`);
}
