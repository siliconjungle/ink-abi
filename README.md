# Ink ABI

One transport contract for Ink's C, Rust, JavaScript, Wasm and GPU backends.

`ink-abi` owns value descriptors, packed numeric layouts, the C value/state headers,
and host execution decisions. It has no compiler or knowledge-store dependency.
Every backend pins this repository. Generated hosts consume its exported sources.

* Records use declaration-order fields; vectors use element order. Packed values
  use little-endian 32-bit words, with low/high words for `u64` and exact IEEE bits
  for `f32`. Lists carry a separate element count. Nested variable-length values
  use tagged value transport rather than the flat compute layout.
* `describe` derives one descriptor from the checked type environment.
  `flat_width` checks target eligibility. Native and JavaScript packing implement
  these descriptors and share conformance fixtures, including malformed inputs.
* `selection::decide` is the only adaptive/static decision implementation. Browser
  hosts execute the same Rust through the included Wasm module. The common
  `features` function computes workload counts from transport/stage descriptors.

Saved prices are untrusted profitability evidence. They bind an exact target key,
compiled scope, execution boundary and observed feature domain. Missing evidence,
out-of-domain inputs or missing CPU estimates retain the compiled CPU baseline.
An alternative needs more than 10% predicted savings. Hosts perform no probe runs.
Runtime owns device failure, resource limits and CPU fallback; it supplies the
eligible candidates. This package grants no mathematical admission authority.

Run `cargo test --locked` with Node available. To rebuild browser decisions, install
`wasm32-unknown-unknown` and run `tools/build-selection.sh`. The committed Wasm and
source fingerprint travel together; CI checks the fingerprint, tests that artifact,
then rebuilds and repeats conformance. A compiler upgrade may change Wasm bytes.

The literal C value ABI includes invocation-local frame tokens. Finishing a frame
retains one returned handle and all handles from before its mark, and discards
other transient handles. Tokens are private to emitted code; callers must not
retain discarded handles. Frames do not change table state, events, journals or
logical snapshot types. Operational implementations remain trusted.

## License

Project code and documentation are [MIT licensed](LICENSE). Third-party dependencies,
vendored projects and research data retain their own licenses and notices.

## JSON and portable values

`value_wire::Schema` defines schema-bound `ink-value-wire-v1` buffers. It encodes,
decodes and validates ordinary declared values with byte/node/depth budgets.
Validated Rust views support borrowed field, sum, text and sequence access;
forward sequence iteration is linear. `source_contract` in `wire_contract`
derives the same signatures for every lowering from serialized checked types.
Temporary owners are explicitly unsupported public wire values.

`json_value` preserves exact numeric tokens and rejects duplicate keys, malformed
Unicode and invalid JSON. `JsonCodec` binds the ordinary JSON knowledge-library
shape; `TypedJsonCodec` in `typed_json` supplies fallible conversions for arbitrary
supported descriptors. Public JSON uses exact decimal conversion for Int/words;
non-finite f32 is binary-only. JavaScript sources expose equivalent codecs, a
bounded byte visitor and `JsonCodec.validateBinary` without a document tree.
JavaScript `ValueCodec.validateValue(value, limits)` checks logical values and
their encoded byte/node/depth budgets without allocating an encoded buffer.
`ValueCodec.capture(value, limits)` additionally constructs an owned canonical
logical tree in that same traversal, equivalent to `decode(encode(value))` for
stable inputs. It never retains caller containers. `ValueCodec.copy(value, limits)`
copies stable, already well-typed values under the caller-trusted contract; it
retains resource accounting but does not validate shapes/ranges. External values
must use `capture` or a checked wire boundary. Aliases in the input may become
separate owned nodes, as in a wire round trip. These are operational mechanisms,
not mathematical rewrite certificates.
Immutable schema edges are resolved once, and single-child validation uses an
iterative traversal. `encode` writes framed values forward into one growing buffer. JSON text
conversion uses logical validation or direct binary emission as appropriate;
neither path constructs and discards a second encoded document.
`JsonCodec` fuses the same encoded byte/node/depth accounting into text parsing
and rendering after checking the complete recursive JSON schema.
Logical JSON rendering accumulates a bounded string directly. It preserves exact
numeric tokens, member order, duplicate rejection and Unicode checks; consumers
may still incur string flattening, so performance evidence includes complete
UTF-8 output consumption as well as returned text.
Value codecs reuse private immutable default resource policies. Each invocation
retains independent counters; arbitrary supplied limits are still normalized and
checked. Encoders copy byte/depth limits into their per-invocation scalar state,
including when an encoder is reused after failure or entered recursively.
Decoder binding traverses the complete recursive descriptor graph to reserve
auxiliary resources. Compact decoding needs a `DataView` only for fixed numeric
lanes; records wider than four fields need a reusable property descriptor.
These flags stay in private codec state and do not mutate the shared plan shape.
All byte, node, depth, tag and Unicode checks remain active.
Single-field record decoders bind the child's reader directly while retaining
both node/depth charges and, for portable values, both frame-end checks in their
original order. Read operations and checked node entrypoints remain separate
internal mechanisms; this does not grant source rewrite authority.
`ValueCodec.visitSpans(bytes, visitor, limits, names)` supplies validated node body
offsets without allocating a byte view per callback. The optional nominal-name
filter suppresses callbacks only; all nodes still undergo structural validation.
Untrusted descriptors use async `createValueCodec`; the synchronous constructor
consumes trusted compiler metadata. Keep transport, codec checking and mathematical
rewrite authority separate. See the distribution's `docs/json-and-portable-values.md`
and run `cargo test --locked`, including cross-language adversarial fixtures.

`createCompiledValueCodec(descriptor, read)` is for trusted emitted materialisers.
Its portable and compact decoders run the complete generic validator before
invoking the literal typed reader synchronously. The reader constructs owned
values with static field names; it is not a validation certificate or a safe API
for arbitrary callbacks. Untrusted schemas continue to use `createValueCodec`.
Buffer/header/schema, Unicode, count and resource checks remain independent of
the generated materialiser, and generic decoding remains a conformance oracle.

### Plain JavaScript object boundary

`JsonCodec.fromJS(value, limits)` captures a plain JavaScript object tree as the
existing library `Json` datatype without serializing or parsing JSON text.
`JsonCodec.toJS(value, options)` returns fresh plain objects and arrays. These
methods are opt-in JavaScript adapters: native Rust/C/Wasm codecs, source types,
wire formats, pure calls and state execution are unchanged. Existing paths do
not invoke the adapters or allocate their traversal state.

Accepted host values are null, booleans, well-formed strings, finite Numbers,
BigInts, dense plain arrays, and plain objects with `Object.prototype` or null
prototype in the current realm. Object fields must be enumerable own string-keyed
data properties. Cycles, sparse/decorated arrays, symbols, accessors, hidden
properties, undefined, functions and class instances reject rather than being
silently dropped or coerced. `toJSON` is never invoked. Repeated references are
copied independently; containers and object identity are not retained. Returned
objects have ordinary prototypes; even `__proto__` is an own data property.
Use declared value codecs for typed-array or other schema-specific inputs.
Callers keep inputs stable. Proxy reflection traps can execute; these adapters
are data validators, not a sandbox for hostile JavaScript code.

Numbers become their JavaScript decimal spelling, with negative zero preserved.
BigInts become exact integer tokens; the `Json` type does not retain a separate
BigInt tag. On return, `numbers: 'number'` is the default and rejects overflow,
underflow and decimal values that would change on Number reserialization.
`numbers: 'bigint'` converts every number to an exact integer, rejecting fractions
and integer magnitudes over 4096 decimal digits. `numbers: 'string'` returns all
numeric tokens unchanged as strings. Numeric token spelling and JSON member
order may be lost in ordinary JS objects; retain `Json` or text when those matter.

Both methods enforce existing portable-value byte/node/depth limits and Unicode
checks, and reject malformed tagged values or duplicate members on return.
The boundary is explicit:

```js
const documents = new JsonCodec(valueCodecs.identity.result);
const document = documents.fromJS({title: 'hello', nested: [true, null]});
const result = callOwned('mark_received', [document]);
if (Object.hasOwn(result, 'Ok')) console.log(documents.toJS(result.Ok));
```

### Agreed-schema JavaScript transport

`valueCodec.compact()` returns a cached `CompactValueCodec` with the exact same
schema identity. Both peers must agree on that identity and
`ink-value-session-v1` before exchanging bytes. The buffer cannot identify its
own schema. This is an explicit transport choice, not a change to
`ink-value-wire-v1`, native ABI values or portable state checkpoints.

A message begins with byte `1`. Records concatenate declaration-order children;
Unit has no payload. Bool and Option/Result discriminants occupy one byte. Words
and raw f32 bits retain the portable format's fixed little-endian representation.
Strings, exact Int, IDs and enums carry a canonical unsigned LEB128 byte length
and UTF-8. Lists/vectors carry a canonical unsigned LEB128 element count. Counts
are at most u32 and use at most five bytes; redundant leading groups reject.
There are no per-node frame lengths or per-message schema hashes. The same hard
byte/node/depth limits apply, counting the version byte and every logical node.

`encode`, `decode` and `validate` check all schema constraints. Decoding returns
owned values and encoding returns a fresh byte buffer. SharedArrayBuffer,
truncated scalars/text, invalid Unicode/tags/counts and trailing bytes reject.
Raw non-finite f32 bits and exact decimal Int survive. The implementation is
currently JavaScript; compiled JavaScript modules expose `callCompact` for pure
functions, with the same preconditions and results as `callBinary`.

These operational codecs are trusted implementations validated by adversarial
fixtures and source/reference comparisons. Tests are not formal codec round-trip
proofs, and a transport negotiation or measurement grants no rewrite authority.

Encoders reuse at most 4096 bytes of private scratch per codec and discard larger
workspaces. Every result is a fresh byte buffer. Reentrant getter calls use a
separate workspace; failed calls release the active workspace. The input value is
never retained. Short ASCII text decoding avoids a temporary byte view; other
text uses the same fatal UTF-8 decoder. Portable headers compare all 40 bytes in
little-endian word groups, using the decoder's existing DataView.

Portable and compact JavaScript decoding lazily bind operations to the complete descriptor
plan on the first materialisation. It uses ordinary closures and a bounded cursor,
without eval or runtime source generation. Every scalar, tag, frame, count and
resource bound remains checked. The generic scanner remains the independent
validation/visitor implementation; malformed-input fixtures compare both paths.
Encoding-only and validation-only users do not construct decoder operations.
The compact validator remains independent of the descriptor-bound decoder and
checks canonical counts even for zero-width Unit collections. Recursive records,
large records, prototype-named fields and owned bit/value carriers are covered.

JavaScript `JsonCodec.parseCompact(text, limits)` and
`TypedJsonCodec.parseCompact(text, limits)` emit agreed-schema transport directly
from JSON tokens without constructing a logical value tree. Record fields emit in
declaration order, arbitrary JSON retains member order and exact number tokens,
and the complete generic compact validator checks the finished bytes. This is
JavaScript transport support, independent of portable checkpoints.

Both forward writers expose `beginCount()` / `endCount(offset, count)`. The compact
writer reserves one prefix byte and grows it to the canonical count only after
the sequence is complete; limits apply to final byte size, with no five-byte
temporary prefix requirement. Every `frame` charges a node and depth, and
`encodeWith` additionally validates complete schema conformance. Callback failure
or reentry cannot reuse a partially active writer. Prefix insertion and record
permutation are trusted bounded byte operations, not algebraic rewrite authority.
