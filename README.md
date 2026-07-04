# corvid

Corvid is an embeddable broker and codec engine for **CVWP**, the Corvid Wire
Protocol — a compact, length-framed binary format for streaming structured
records between services (flow telemetry, metrics fan-in, capture replay).

It is a library first; `corvidctl` is a thin CLI over the same API.

## What's in the box

- **Framing & parsing** (`parser`) — zero-copy message framing over the CVWP
  stream header.
- **Schemas & templates** (`schema`) — versioned schema/template registries, a
  well-known information model, on-wire layout computation and a persistable
  catalog.
- **Codecs** (`codec`) — RLE, zig-zag delta, canonical Huffman, an LZSS coder, a
  shared-dictionary substitution coder and a self-describing compressed-frame
  container.
- **Reassembly** (`reassembly`) — out-of-order fragment reassembly, a sliding
  window buffer, coverage tracking and overlap-resolution policy.
- **Flows** (`flow`) — the 5-tuple flow table (arena-backed), connection
  registry, bidirectional pairing, expiry policy and derived per-flow stats.
- **Transform engine** (`engine`) — a small bounded stack VM with an assembler,
  disassembler, symbol table, lexical scopes and an execution tracer.
- **Analytics** (`analytics`) — histograms, top-N, approximate quantiles,
  entropy, rate meters, bucketed time series and a labeled counter registry.
- **Filtering & query** (`filter`, `query`) — a small boolean filter language
  with constant folding, plus a query layer with projection, ordering and
  aggregates.
- **Sessions** (`session`) — the front door that drives every subsystem from a
  parsed stream, with a multi-session broker, message router and resource
  quotas.

## Building

```
cargo build --release
cargo test
```

CVWP tooling:

```
corvidctl inspect capture.cvwp        # human-readable message listing
corvidctl run capture.cvwp            # feed a stream through a session
corvidctl filter 'octets > 1000' in.cvwp
corvidctl asm transform.s             # assemble VM bytecode
```

## Protocol sketch

```
stream:  magic="CVWP" | version | flags | msg_count | messages...
message: type | flags | length(u32) | payload[length]
```

See `src/wire.rs` for the message-type table and `src/proto.rs` for the
per-type descriptors (category, minimum length, session-open requirement).

## Fuzzing

The `fuzz/` crate contains libFuzzer targets driven under AddressSanitizer:

```
cargo +nightly fuzz run session_fuzzer -- -dict=fuzz/corvid.dict
```

Targets: `frame_fuzzer` (framing), `session_fuzzer` (full session state
machine) and `codec_fuzzer` (decompressors). Seed corpora and the dictionary
are regenerated with `cargo run --example seedgen -- fuzz`.

