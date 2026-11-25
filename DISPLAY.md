# The display, written a second time

`src/display.rs` is Codama's instruction display written again, in Rust. Given a Codama IDL with its display nodes and an instruction, it returns the intent, the sentence and the fields that `@codama/dynamic-instructions` 1.0.0 returns. The two share no code.

A specification with one implementation is whatever that implementation does. A second one shows where the text decides and where the code does, and that list is what this page is for.

## How far the two agree

| Compared on | Instructions | Same display |
| --- | --- | --- |
| The 17 cases (`cargo test --test display`) | 54 | 54 |
| The probes (`cargo test --test probes`) | 318 | 310, and 8 differences kept on purpose |
| Four Mainnet blocks kept in `real/survey` (`cargo run --release --bin compare`) | 6,435 | 6,435 |
| Forty Mainnet blocks, 2026-11-24 18:03 to 2026-11-25 06:37 UTC | 75,938 | 75,938 |

The forty blocks hold 16,272 transactions, 173 of them votes only and left out. Every instruction of a program with an IDL here was compared, top-level and inner (34,230 are inner): Token 22,850, Compute Budget 16,349, System 15,951, Token-2022 12,920, Associated Token Account 6,402, Jupiter 1,456, Memo 5, Stake 5. Codama shows 75,042 of them and nothing for the other 896, and the Rust display shows the same and nothing for the same. `real/survey-day.json` lists the slots; `node tools/codama/survey.mjs --slots <them>` takes them again.

The probes are small IDLs written for the purpose (`tools/codama/probes.mjs`), one per behaviour that the real programs do not reach: each type node, each display node, each way to find an instruction, and data that is short, long or not in its usual form. They found four mistakes in the Rust display that the real instructions had not, and the forty blocks found a fifth that the cases had not (a constant with padding, in every Token-2022 mint that has extensions).

## Where the two differ, on purpose

Eight probes, four reasons, all in `tests/probe-differences.json`. In each, the stable release gives a wrong value, an empty one or none. For six of the eight the 2.0 release candidate already gives what the Rust display gives, and the test checks that it does.

| What | Stable release | Rust display |
| --- | --- | --- |
| A big-endian number (5 probes) | Read low byte first, whatever `endian` says: `01 00` as a big-endian `u16` reads 1. A big-endian tag is written the same way, so its instruction is never found | 256, and the instruction is found |
| A type that holds itself, such as a list node with an optional next node | A stack overflow, for every instruction that uses the type | Read as far as the data goes |
| A number whose display node is of a kind it does not know | A field with a label and no value | The number |
| Arguments whose names are not in camel case | Fields with empty values | The values |

## Where the stable release and the specification part, and the Rust display stays with the release

The expected displays of the cases are the stable release's, so these are kept as they are. The release candidate changes each of them.

1. **The payload of an enum variant is never shown.** `enumVariantDisplayNode.skipInnerData` hides it "when true"; the stable release hides it always. A variant `circle { radius: 5 }` reads "Circle".
2. **An optional account that is absent is shown.** With the `programId` strategy the program's own address stands in for it, and the stable release shows that address under the account's label.
3. **With the `omitted` strategy, names are given by position.** One optional account left out, and every account after it carries the label of another. The case `confidential-withdraw-omitted-accounts` shows it on the official Token-2022 IDL: the authority reads "Range Record". The release candidate names none of them ("Unidentified Account #1"). Which accounts are present follows from the instruction's data there, and no node can say so.
4. **`flatten` on a field of a flattened struct is ignored.** The inner struct is shown as JSON.
5. **Accounts after the named ones are not shown** unless the instruction has `remainingAccounts` (case `trezor-6843-co-signed-transfer`).

## What the specification leaves open

What the stable release does where the text is silent, and the Rust display with it. The name of a probe is given where one alone shows the point.

### Reading the data

- **An option whose tag is neither 0 nor 1** is absent, and the bytes of its item are not skipped: they are read as what comes next (`a value after an option with a tag of 2`).
- **A short `u16` in more bytes than it needs** is accepted (`80 00` is 0); one above 65535 is refused.
- **Bytes after the last argument** are ignored.
- **A counted list with nothing left to read, not even its count,** is an empty list.
- **A fixed size smaller than the number in it** pads the number with zeros; a size prefix smaller than the number refuses it.
- **Text**: bytes that are not UTF-8 become U+FFFD, a leading byte order mark is dropped, and every NUL is removed, in the middle of the text too.
- **A struct with the same field name twice** keeps the last value, at the first one's place; **a map with the same key twice** keeps the last value. Keys that read as array indices come first, in increasing order.
- **An enum tag that names no variant, a list longer than the data**: nothing is shown for the whole instruction.
- **Two instructions that match the same data**: the first. **A program with one instruction and no discriminator**: that one. Two of them: nothing.
- **A program that comes along in another IDL** (`additionalPrograms`) is found there.
- **A node with a part missing** (a boolean without its size, a list without its count) and **a link to a type that is not there** throw, also when the data never reaches them: the decoder is built for the whole instruction first (`a part missing in a type the data does not reach`).

### Showing it

- **Labels made from names.** `minSOLAmount` reads "Min S O L Amount" in the stable release and "Min Sol Amount" in the release candidate: the rule is nowhere, and two releases already give two labels.
- **Numbers.** Integers of 64 bits and more are exact. Floating-point numbers read as JavaScript prints them: `1e+100`, `NaN`, `Infinity`, `0` for minus zero, `0.10000000149011612` for the `f32` nearest to 0.1.
- **Bytes** read `["base64","AQL/"]`. **Structs, tuples, sets and maps** read as JSON, with JavaScript's own marks: a map whose keys are structs reads `{"[object Object]":9}`.
- **Lists** are joined with ", ". A list of lists reads as one flat list: `1, 2, 3, 4`.
- **A list of addresses** gives one field per address, numbered when there are several; an empty one gives no field at all. An empty list of anything else gives a field with an empty value. A set of addresses is not a list: it reads as JSON.
- **An absent option** reads "none" ("None" in the release candidate).
- **Amounts**: no separator for thousands, zeros dropped at the end, the unit after a space, no unit if it is empty or does not resolve. Decimals that do not resolve, are not whole or are negative give `12345 (raw)`, and then the instruction has no sentence at all.
- **Dates** are ISO 8601 in UTC with milliseconds, as in the specification's examples; a part of a millisecond is dropped. A year after 9999 reads `+010000-01-01T00:00:00.000Z`. Past the last date JavaScript knows (year 275760), the raw number is shown.
- **Durations** read `HH:MM:SS` with hours that do not stop at 24 (`100:01:01`); a part of a second is dropped; a negative one, or zero ticks per second, gives the raw number. Above 2^53 seconds the arithmetic is in doubles: 2^64 - 1 seconds ends in `:16`, one more than the true value.
- **Slices of text** count UTF-16 units, where the specification says "the decoded character sequence": an emoji counts for two, and a slice through one leaves half a character, which is not Unicode any more (`text sliced in the middle of an emoji`; the Rust display gives U+FFFD there). Negative indices count from the end, though the specification types them as unsigned.
- **Sentences.** Spaces are allowed inside `${ }`. Anything that is not `data.name` or `accounts.name` is left as written; names are matched as written, labels are not names. A value that itself holds `${...}` is not expanded again. If one placeholder has no value, the instruction has no sentence.
- **Provided values.** Of two `providedNode` with one name, the last wins. A provided value may point to another, and one that points to itself resolves to nothing.
- **Account state.** Decimals and units are read from whatever account the instruction lists, whoever owns it and however long it is (`decimals from an account that another program owns`). One that is too short for its layout makes the whole display throw.
- **Remaining accounts in several groups.** A group takes accounts as long as they fit its `isSigner`, and the last group takes all that is left: an account that does not sign can read "Signers #1" (`remaining accounts that are not signers, in a single group of signers`).

## What is not there

- **The 2.0 release candidate.** It is a rewrite, and its text differs on most values (`None`, `Empty`, structs as `X: 1, Y: 2`). The cases keep what it gives in `expected.displayNext`; the Rust display will follow 2.0 once it is released.
- **A constant written as a map**, and offsets inside a constant other than padding and the ones that stay within its own bytes. No IDL here has either.
- Events, PDAs and resolvers, which the instruction display does not use.
