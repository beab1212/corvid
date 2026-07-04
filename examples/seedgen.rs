//! Generates seed corpora and a libFuzzer dictionary for the fuzz targets.
//!
//! Run with `cargo run --example seedgen -- fuzz` to (re)populate
//! `fuzz/seeds/<target>` and write `fuzz/corvid.dict`. The seeds are
//! well-formed CVWP streams built with the same [`StreamBuilder`] the tests
//! use, giving the fuzzer valid starting points to mutate from. Point a fuzz
//! run at them with e.g. `cargo +nightly fuzz run session_fuzzer fuzz/seeds/session_fuzzer`.

use std::fs;
use std::path::{Path, PathBuf};

use corvid::codec::rle;
use corvid::parser::builder::payload;
use corvid::parser::StreamBuilder;
use corvid::wire::MsgType;

fn write_seed(dir: &Path, name: &str, bytes: &[u8]) {
    let _ = fs::create_dir_all(dir);
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn frame_seeds(root: &Path) {
    let dir = root.join("seeds/frame_fuzzer");

    write_seed(&dir, "empty_stream", &StreamBuilder::new().build());

    write_seed(
        &dir,
        "session_only",
        &StreamBuilder::new().msg(MsgType::SessionOpen, payload::session_open(1, 0)).build(),
    );

    let full = StreamBuilder::new()
        .msg(MsgType::SessionOpen, payload::session_open(1, 0))
        .msg(MsgType::FlowOpen, payload::flow_open(7, 4096))
        .msg(MsgType::DataRecord, payload::data_record(0, 7, 0x0a000001, 0x0a000002, 1500, 3))
        .msg(MsgType::SessionClose, payload::session_close(1))
        .build();
    write_seed(&dir, "session_flow_data", &full);
}

fn session_seeds(root: &Path) {
    let dir = root.join("seeds/session_fuzzer");

    // A schema definition followed by a matching data record.
    let mut schema_payload = Vec::new();
    schema_payload.extend_from_slice(&1u16.to_be_bytes()); // schema id
    schema_payload.extend_from_slice(&2u16.to_be_bytes()); // field count
    schema_payload.extend_from_slice(&[0x00, 0x01, 0x03, 0x00, 0x00]); // id=1 u32
    schema_payload.extend_from_slice(&[0x00, 0x02, 0x02, 0x00, 0x00]); // id=2 u16

    let s = StreamBuilder::new()
        .msg(MsgType::SessionOpen, payload::session_open(1, 0))
        .msg(MsgType::SchemaDef, schema_payload)
        .msg(MsgType::FlowOpen, payload::flow_open(3, 8192))
        .build();
    write_seed(&dir, "schema_flow", &s);

    // Fragment reassembly path.
    let frag = StreamBuilder::new()
        .msg(MsgType::SessionOpen, payload::session_open(2, 0))
        .msg(MsgType::FlowOpen, payload::flow_open(9, 4096))
        .msg(MsgType::Fragment, payload::fragment(9, 0, b"hello "))
        .msg(MsgType::Fragment, payload::fragment(9, 6, b"world"))
        .build();
    write_seed(&dir, "fragments", &frag);

    // VM module load + call.
    let bc = corvid::engine::assembler::assemble("push 2\npush 3\nadd\nret").unwrap_or_default();
    let mut modload = Vec::new();
    modload.extend_from_slice(&1u32.to_be_bytes());
    modload.extend_from_slice(&bc);
    let vm = StreamBuilder::new()
        .msg(MsgType::SessionOpen, payload::session_open(3, 0))
        .msg(MsgType::ModuleLoad, modload)
        .build();
    write_seed(&dir, "module_load", &vm);
}

fn codec_seeds(root: &Path) {
    let dir = root.join("seeds/codec_fuzzer");
    // selector byte + payload for each codec branch.
    let rle_payload = rle::encode(b"aaaaaaaabbbbcccccccc");
    let mut rle_seed = vec![0u8];
    rle_seed.extend_from_slice(&rle_payload);
    write_seed(&dir, "rle", &rle_seed);

    let lz_payload = corvid::codec::lz::encode(b"abcabcabcabcabcabc");
    let mut lz_seed = vec![1u8];
    lz_seed.extend_from_slice(&lz_payload);
    write_seed(&dir, "lz", &lz_seed);

    write_seed(&dir, "roundtrip", b"\x04the quick brown fox");
}

fn write_dictionary(root: &Path) {
    // Tokens the mutator can splice in: the magic, message-type bytes and a few
    // structurally-significant values.
    let mut dict = String::new();
    dict.push_str("# Corvid Wire Protocol fuzzing dictionary\n");
    dict.push_str("magic=\"CVWP\"\n");
    dict.push_str("version=\"\\x04\"\n");
    for v in 0x01u8..=0x24 {
        dict.push_str(&format!("msgtype_{v:02x}=\"\\x{v:02x}\"\n"));
    }
    dict.push_str("len_zero=\"\\x00\\x00\\x00\\x00\"\n");
    dict.push_str("len_one=\"\\x00\\x00\\x00\\x01\"\n");
    dict.push_str("cap_magic=\"CVCP\"\n");
    dict.push_str("catalog_magic=\"CVCT\"\n");
    fs::write(root.join("corvid.dict"), dict).expect("write dict");
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "fuzz".to_string());
    let root = PathBuf::from(arg);
    frame_seeds(&root);
    session_seeds(&root);
    codec_seeds(&root);
    write_dictionary(&root);
    println!("seed corpora and dictionary written under {}", root.display());
}
