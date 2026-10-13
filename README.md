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
Immutable schema edges are resolved once, and single-child validation uses an
iterative traversal. `encode` writes framed values forward into one growing buffer. JSON text
conversion uses logical validation or direct binary emission as appropriate;
neither path constructs and discards a second encoded document.
`JsonCodec` fuses the same encoded byte/node/depth accounting into text parsing
and rendering after checking the complete recursive JSON schema.
`ValueCodec.visitSpans(bytes, visitor, limits, names)` supplies validated node body
offsets without allocating a byte view per callback. The optional nominal-name
filter suppresses callbacks only; all nodes still undergo structural validation.
Untrusted descriptors use async `createValueCodec`; the synchronous constructor
consumes trusted compiler metadata. Keep transport, codec checking and mathematical
rewrite authority separate. See the distribution's `docs/json-and-portable-values.md`
and run `cargo test --locked`, including cross-language adversarial fixtures.

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
