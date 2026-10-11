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
