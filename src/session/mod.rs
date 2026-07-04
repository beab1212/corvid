//! The session layer: the front door of the engine.
//!
//! A [`Session`] owns one instance of every subsystem and drives them from a
//! parsed message stream. It is the integration point where protocol state
//! accumulates: schemas and templates are registered, data records fold into
//! the flow table, fragments reassemble, and control messages drive the VM and
//! connection machinery.
//!
//! A session is single-threaded and cheap to create; the fuzz harnesses spin up
//! a fresh one per input so no state leaks between iterations.

pub mod broker;
pub mod decode;
pub mod limits;
pub mod router;

pub use broker::Broker;
pub use limits::{LimitTracker, Limits, Resource};
pub use router::Router;

use std::collections::HashMap;

use crate::alloc::RegionPool;
use crate::analytics::FlowSummary;
use crate::codec::{Codec, CompressorState};
use crate::config::Config;
use crate::engine::{Modules, ScopeStack, SymbolTable, Vm};
use crate::error::{Error, Result};
use crate::flow::{BiflowPairer, ConnRegistry, FlowTable};
use crate::metrics::Metrics;
use crate::parser::{FrameParser, Message};
use crate::reassembly::{Reassembler, Window};
use crate::schema::field::FieldType;
use crate::schema::registry::SchemaRegistry;
use crate::schema::template::{Template, TemplateCache};
use crate::util::ByteReader;
use crate::wire::MsgType;

/// Per-stream sliding-window plus reassembly context, created on `STREAM_OPEN`
/// / `FLOW_OPEN` and torn down on timeout.
struct StreamCtx {
    window: Window,
    read_pos: usize,
}

pub struct Session {
    cfg: Config,
    clock: u64,
    opened: bool,
    session_id: u32,

    schemas: SchemaRegistry,
    templates: TemplateCache,
    flows: FlowTable,

    summary: FlowSummary,

    biflows: BiflowPairer,

    reasm: HashMap<u32, Reassembler>,
    streams: HashMap<u32, StreamCtx>,

    conns: ConnRegistry,

    modules: Modules,
    symbols: SymbolTable,
    scopes: ScopeStack,
    vm_result: i64,

    compressor: CompressorState,
    channels: HashMap<u32, Vec<u8>>,
    /// Fast-path handle to the most recently touched channel's backing store:
    /// the channel id and a pointer to its first byte, where the broker stamps a
    /// reuse generation without re-hashing the channel map.
    chan_mark: Option<(u32, *mut u8)>,
    sections: Vec<RegionPool>,
    /// A snapshot in progress: base pointer and capacity of the section region
    /// a `SNAPSHOT_BEGIN` latched onto, to be written back on `SNAPSHOT_COMMIT`.
    pending_snap: Option<SnapTarget>,

    metrics: Metrics,
}

/// A latched write-back target for the two-phase snapshot protocol.
#[derive(Clone, Copy)]
struct SnapTarget {
    base: *mut u8,
    cap: usize,
}

impl Session {
    pub fn new() -> Session {
        Session::with_config(Config::default())
    }

    pub fn with_config(cfg: Config) -> Session {
        let templates = TemplateCache::new(cfg.template_capacity);
        let flows = FlowTable::new(cfg.flow_capacity, cfg.flow_idle_ticks, cfg.arena_chunk);
        let compressor = CompressorState::new(cfg.window_size * 4);
        Session {
            clock: 1,
            opened: false,
            session_id: 0,
            schemas: SchemaRegistry::new(),
            templates,
            flows,
            summary: FlowSummary::new(16),
            biflows: BiflowPairer::new(),
            reasm: HashMap::new(),
            streams: HashMap::new(),
            conns: ConnRegistry::new(cfg.window_size.min(4096)),
            modules: Modules::new(),
            symbols: SymbolTable::new(),
            scopes: ScopeStack::new(),
            vm_result: 0,
            compressor,
            channels: HashMap::new(),
            chan_mark: None,
            sections: Vec::new(),
            pending_snap: None,
            metrics: Metrics::new(),
            cfg,
        }
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn is_open(&self) -> bool {
        self.opened
    }

    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }

    /// Parse one CVWP stream and process every message it contains.
    pub fn process_stream(&mut self, data: &[u8]) -> Result<()> {
        self.metrics.note_stream(data.len());
        let mut parser = FrameParser::new();
        let messages = parser.parse_all(data)?;
        for msg in messages {
            if msg.is_continuation() {
                std::hint::black_box(parser.slice_payload(data, &msg));
            }
            self.clock += 1;
            self.metrics.note_message(msg.ty as u8);
            if let Err(e) = self.dispatch(&msg) {
                self.metrics.note_error();
                if !e.kind().is_recoverable() {
                    return Err(e);
                }
            }
        }
        self.metrics.arena_high_water = self.flows.arena_high_water();
        Ok(())
    }

    fn require_open(&self) -> Result<()> {
        if self.opened {
            Ok(())
        } else {
            Err(Error::protocol("message before SESSION_OPEN"))
        }
    }

    fn dispatch(&mut self, msg: &Message) -> Result<()> {
        use MsgType::*;
        match msg.ty {
            SessionOpen => self.on_session_open(msg.payload),
            SessionClose => self.on_session_close(msg.payload),
            SchemaDef => self.on_schema_def(msg.payload, false),
            SchemaUpdate => self.on_schema_def(msg.payload, true),
            TemplateDef => self.on_template_def(msg.payload),
            DataRecord => self.on_data_record(msg.payload),
            Fragment => self.on_fragment(msg.payload),
            FragmentReorder => self.on_fragment_reorder(msg.payload),
            FlowOpen => self.on_flow_open(msg.payload),
            FlowTimeout => self.on_flow_timeout(msg.payload),
            CompressedBlock => self.on_compressed_block(msg.payload),
            SectionHeader => self.on_section_header(msg.payload),
            ConnOpen => self.on_conn_open(msg.payload),
            ConnReset => self.on_conn_reset(msg.payload),
            Continuation | NestedRecord => self.on_nested(msg.payload, 0),
            ModuleLoad => self.on_module_load(msg.payload, false),
            ModuleReload => self.on_module_load(msg.payload, true),
            CallSymbol => self.on_call_symbol(msg.payload),
            ScopeBegin => self.scopes.begin(),
            PluginRegister => self.on_plugin_register(msg.payload),
            ScopeEnd => self.scopes.end(),
            PluginInvoke => self.on_plugin_invoke(msg.payload),
            CompressConfig => self.on_compress_config(msg.payload),
            CompressData => self.on_compress_data(msg.payload),
            Request => self.on_request(msg.payload),
            ProcessQueue => {
                self.conns.process_queue();
                Ok(())
            }
            StreamOpen => self.on_stream_open(msg.payload),
            Packet => self.on_packet(msg.payload),
            ReadWindow => self.on_read_window(msg.payload),
            SeekRelative => self.on_seek_relative(msg.payload),
            ChannelOpen => self.on_channel_open(msg.payload),
            ChannelTeardown => self.on_channel_teardown(msg.payload),
            ChannelAlloc => self.on_channel_alloc(msg.payload),
            SnapshotBegin => self.on_snapshot_begin(msg.payload),
            SnapshotCommit => self.on_snapshot_commit(msg.payload),
        }
    }

    // --- lifecycle -------------------------------------------------------

    fn on_session_open(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = ByteReader::new(payload);
        self.session_id = r.u32()?;
        let _features = r.u32().unwrap_or(0);
        self.opened = true;
        Ok(())
    }

    fn on_session_close(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = ByteReader::new(payload);
        let _sid = r.u32().unwrap_or(self.session_id);
        // Reclaim arena pages first so idle flows fold into the close summary
        // without retaining duplicate copies of their records.
        self.flows.abandon_arena();
        self.flows.flush();
        self.opened = false;
        Ok(())
    }

    // --- schema / template ----------------------------------------------

    fn on_schema_def(&mut self, payload: &[u8], update: bool) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let schema_id = r.u16()?;
        let fields = decode::parse_fields(&mut r)?;
        if update {
            self.schemas.update(schema_id, fields)?;
        } else {
            self.schemas.define(schema_id, fields)?;
        }
        Ok(())
    }

    fn on_template_def(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let template_id = r.u16()?;
        let schema_id = r.u16()?;
        let fields = decode::parse_fields(&mut r)?;
        let tmpl = Template::new(template_id, schema_id, fields);
        let evicted = self.templates.define(tmpl);
        self.metrics.templates_defined += 1;
        // Drop flow bindings to any template we just evicted so nothing keeps a
        // stale reference to it.
        for id in evicted {
            self.flows.purge_template(id);
        }
        Ok(())
    }

    fn on_data_record(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let hdr = decode::parse_data_header(&mut r)?;
        let body = r.rest();

        let (gen, field_count, row_stride, schema_id) = {
            let tmpl = self
                .templates
                .get(hdr.template_id)
                .ok_or_else(|| Error::unresolved("data for unknown template"))?;
            (tmpl.generation, tmpl.field_count() as u16, tmpl.row_stride, tmpl.schema_id)
        };

        self.metrics.note_record();
        self.summary
            .octet_hist
            .record_shifted(hdr.octets, field_count as u32);
        self.summary
            .top_sources
            .add_and_sift(hdr.key.src as u64, hdr.octets);
        if self.cfg.aggregate {
            self.flows.update(
                hdr.key,
                hdr.octets,
                hdr.packets,
                hdr.template_id,
                gen,
                field_count,
                self.clock,
            );
            let mut snap = crate::flow::FlowRecord::new(hdr.key, self.clock);
            snap.octets = hdr.octets;
            snap.packets = hdr.packets;
            self.biflows.observe_with_stride(&snap, row_stride);
        }
        // Materialise the first fixed-width column when present.
        if !body.is_empty() {
            if let Some(tmpl) = self.templates.get(hdr.template_id) {
                if let Some(f) = tmpl.fields.first() {
                    if f.ty == FieldType::Fixed {
                        let mut br = ByteReader::new(body);
                        let _ = crate::decode::value::decode_field(f, &mut br)?;
                    }
                }
            }
        }
        // If the template's schema is registered, materialise the fixed-width
        // row prefix so downstream consumers can index columns positionally.
        if !body.is_empty() {
            if let Some(schema) = self.schemas.get(schema_id) {
                if let Ok(row) =
                    crate::decode::record::pack_fixed_prefix(&schema.fields, row_stride, body)
                {
                    self.metrics.rows_packed += 1;
                    let _ = row;
                }
            }
        }
        self.flows.sweep(self.clock);
        Ok(())
    }

    // --- reassembly ------------------------------------------------------

    fn on_flow_open(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let flow_id = r.u32()?;
        let window = r.u32().unwrap_or(self.cfg.window_size as u32) as usize;
        self.reasm
            .entry(flow_id)
            .or_insert_with(|| Reassembler::new(self.cfg.max_fragments, self.cfg.window_size * 4));
        self.streams.entry(flow_id).or_insert_with(|| StreamCtx {
            window: Window::new(window.clamp(64, self.cfg.window_size * 4)),
            read_pos: 0,
        });
        self.metrics.open_flow();
        Ok(())
    }

    fn on_fragment(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let flow_id = r.u32()?;
        let offset = r.i32()?;
        let len = r.u16()? as usize;
        let data = r.take(len)?;
        let re = self
            .reasm
            .get_mut(&flow_id)
            .ok_or_else(|| Error::unresolved("fragment for unopened flow"))?;
        re.add(offset, data)?;
        self.metrics.fragments_seen += 1;
        Ok(())
    }

    fn on_fragment_reorder(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let flow_id = r.u32()?;
        let start = r.i32()?;
        let end = r.i32()?;
        let re = self
            .reasm
            .get_mut(&flow_id)
            .ok_or_else(|| Error::unresolved("reorder for unopened flow"))?;
        re.reorder(start, end)?;
        Ok(())
    }

    fn on_flow_timeout(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let flow_id = r.u32()?;
        if let Some(re) = self.reasm.remove(&flow_id) {
            let _ = re.reassemble()?;
        }
        self.streams.remove(&flow_id);
        self.metrics.close_flow();
        Ok(())
    }

    // --- codec -----------------------------------------------------------

    fn on_compressed_block(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let codec = Codec::from_code(r.u8()?)?;
        let declared = r.u64()?;
        let limit = r.u32()? as usize;
        let data = r.rest();
        let lim = limit.clamp(1, self.cfg.window_size * 16);
        if declared != 0 {
            let n = self.compressor.inflate_declared(codec, data, declared, lim)?;
            self.metrics.note_codec(data.len(), n);
        } else {
            let out = crate::codec::compress::inflate_block(codec, data, lim)?;
            self.metrics.note_codec(data.len(), out.len());
        }
        Ok(())
    }

    fn on_compress_config(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let codec = r.u8()?;
        let limit = r.u32().unwrap_or(0) as usize;
        let codec_enum = Codec::from_code(codec)?;
        let stride = if codec_enum == Codec::Delta {
            r.u32().unwrap_or(0) as usize
        } else {
            0
        };
        self.compressor.configure_ext(codec, limit, stride)?;
        if codec_enum == Codec::Dict {
            let mut entries = Vec::new();
            while r.remaining() >= 2 {
                let elen = r.u16()? as usize;
                if elen > r.remaining() {
                    break;
                }
                entries.push(r.take(elen)?.to_vec());
            }
            self.compressor.configure_dict(entries);
        }
        Ok(())
    }

    fn on_compress_data(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let out = self.compressor.inflate(payload)?;
        self.metrics.note_codec(payload.len(), out.len());
        Ok(())
    }

    // --- sections (columnar region) -------------------------------------

    fn on_section_header(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let count = r.u32()? as usize;
        let stride = r.u32()? as usize;
        if count == 0 || stride == 0 {
            return Err(Error::malformed("empty section"));
        }
        if count > 1 << 20 || stride > 1 << 16 {
            return Err(Error::limit("section dims too large"));
        }
        let body = r.rest();
        // A section whose body already spans the full declared grid is copied in
        // one shot; a sparse section is filled row by row.
        let pool = if body.len() >= stride && body.len() % stride == 0 {
            RegionPool::packed(count, stride, body)?
        } else {
            let mut pool = RegionPool::with_dims(count, stride)?;
            let rows = (body.len() / stride).min(count);
            for i in 0..rows {
                pool.write_row(i, &body[i * stride..(i + 1) * stride])?;
            }
            pool
        };
        if self.sections.len() >= 8 {
            self.sections.remove(0);
        }
        self.sections.push(pool);
        Ok(())
    }

    // --- connections -----------------------------------------------------

    fn on_conn_open(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        self.conns.open(id);
        Ok(())
    }

    fn on_conn_reset(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        self.conns.reset(id);
        Ok(())
    }

    fn on_request(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        let opcode = r.u16()?;
        let body = r.rest();
        self.conns.enqueue(id, opcode, body)
    }

    // --- transform engine ------------------------------------------------

    fn on_module_load(&mut self, payload: &[u8], reload: bool) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let module_id = r.u32()?;
        let code = r.rest().to_vec();
        if reload {
            self.modules.reload(module_id, code)?;
        } else {
            self.modules.load(module_id, code);
        }
        Ok(())
    }

    fn on_call_symbol(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let sym_id = r.u32()?;
        let input = r.u64().unwrap_or(0) as i64;
        // A symbol may be defined implicitly by a module-relative descriptor:
        // sym_id high 16 bits = module, low 16 = offset. Register on demand.
        let module_id = sym_id >> 16;
        let offset = sym_id & 0xFFFF;
        if self.symbols.resolve(sym_id).is_err() {
            if let Some(code) = self.modules.get(module_id) {
                let len = code.len().saturating_sub(offset as usize) as u32;
                self.symbols.define(sym_id, module_id, offset, len);
            }
        }
        let mut vm = Vm::new();
        vm.set_input(0, input);
        self.vm_result = vm.run(&mut self.modules, &self.symbols, sym_id)?;
        Ok(())
    }

    fn on_plugin_register(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let plugin_id = r.u32()?;
        let handler = r.u16()?;
        self.scopes.register(plugin_id, handler)
    }

    fn on_plugin_invoke(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let plugin_id = r.u32()?;
        let _handler = self.scopes.invoke(plugin_id)?;
        Ok(())
    }

    // --- streams ---------------------------------------------------------

    fn on_stream_open(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let stream_id = r.u32()?;
        let window = r.u32().unwrap_or(self.cfg.window_size as u32) as usize;
        self.streams.entry(stream_id).or_insert_with(|| StreamCtx {
            window: Window::new(window.clamp(64, self.cfg.window_size * 4)),
            read_pos: 0,
        });
        Ok(())
    }

    fn on_packet(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let stream_id = r.u32()?;
        let seq = r.u64()?;
        let data = r.rest();
        let ctx = self
            .streams
            .get_mut(&stream_id)
            .ok_or_else(|| Error::unresolved("packet for unopened stream"))?;
        ctx.window.place(seq, data)
    }

    fn on_read_window(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let stream_id = r.u32()?;
        let seq = r.u64()?;
        let len = r.u32()? as usize;
        let ctx = self
            .streams
            .get_mut(&stream_id)
            .ok_or_else(|| Error::unresolved("read for unopened stream"))?;
        // Sample the resume marker under the current cursor before serving the
        // read; it lets a caller tell whether the window still holds live data.
        let marker = ctx.window.peek_cursor(ctx.read_pos);
        std::hint::black_box(marker);
        let out = ctx.window.read(seq, len)?;
        ctx.read_pos = ctx.read_pos.wrapping_add(out.len());
        Ok(())
    }

    fn on_seek_relative(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let stream_id = r.u32()?;
        let delta = r.i32()?;
        let ctx = self
            .streams
            .get_mut(&stream_id)
            .ok_or_else(|| Error::unresolved("seek for unopened stream"))?;
        let next = ctx.read_pos as i64 + delta as i64;
        if next < 0 || next as usize > ctx.window.size() {
            return Err(Error::malformed("stream seek out of range"));
        }
        ctx.read_pos = next as usize;
        Ok(())
    }

    // --- channels --------------------------------------------------------

    fn on_channel_open(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        let size = (r.u32()? as usize).clamp(1, 1 << 20);
        let buf = self.channels.entry(id).or_insert_with(|| vec![0u8; size]);
        // Remember where this channel lives so the next stamp can skip the map.
        self.chan_mark = Some((id, buf.as_mut_ptr()));
        self.metrics.live_channels += 1;
        Ok(())
    }

    fn on_channel_teardown(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        if self.channels.remove(&id).is_some() {
            self.metrics.live_channels -= 1;
        }
        Ok(())
    }

    fn on_channel_alloc(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let id = r.u32()?;
        let size = (r.u32()? as usize).clamp(1, 1 << 20);
        let hot = matches!(self.chan_mark, Some((mark_id, _)) if mark_id == id);
        if let Some(buf) = self.channels.get_mut(&id) {
            buf.resize(size, 0);
        } else {
            self.channels.insert(id, vec![0u8; size]);
            self.metrics.live_channels += 1;
        }
        // Re-stamp the reuse generation. When this channel is the marked hot one
        // we take the fast path and reach it through the cached pointer.
        if hot {
            if let Some((_, ptr)) = self.chan_mark {
                stamp_channel(ptr, self.clock);
            }
        }
        Ok(())
    }

    // --- nested records --------------------------------------------------

    fn on_nested(&mut self, payload: &[u8], depth: u32) -> Result<()> {
        self.require_open()?;
        if depth > 8 {
            return Err(Error::limit("nested record too deep"));
        }
        if depth == 0 && payload.first().copied().unwrap_or(0) == 0x0F {
            let _ = crate::decode::structured::decode_structured(payload)?;
            return Ok(());
        }
        let mut r = ByteReader::new(payload);
        let inner_ty = r.u8()?;
        let body = r.rest();
        match MsgType::from_u8(inner_ty) {
            Some(MsgType::DataRecord) => self.on_data_record(body),
            Some(MsgType::Continuation) | Some(MsgType::NestedRecord) => {
                self.on_nested(body, depth + 1)
            }
            Some(_) | None => Ok(()),
        }
    }

    // --- snapshots -------------------------------------------------------

    /// Phase one of a snapshot: latch the backing region of an existing section
    /// so the committed image can be written straight into it, avoiding a copy.
    fn on_snapshot_begin(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let idx = r.u32()? as usize;
        let sec = self
            .sections
            .get(idx)
            .ok_or_else(|| Error::unresolved("snapshot for unknown section"))?;
        self.pending_snap = Some(SnapTarget { base: sec.base_ptr(), cap: sec.total_bytes() });
        Ok(())
    }

    /// Phase two: write the committed bytes into the latched section region.
    fn on_snapshot_commit(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let len = r.u32()? as usize;
        let body = r.take(len.min(r.remaining()))?;
        if let Some(target) = self.pending_snap.take() {
            write_snapshot(target, body);
        }
        if body.starts_with(b"CVSS") {
            let _ = crate::snapshot::format::decode(body)?;
        }
        Ok(())
    }

    /// Flush and finalise the session (mirrors what the harness does at teardown).
    pub fn teardown(&mut self) {
        self.flows.flush();
        self.reasm.clear();
        self.streams.clear();
        self.channels.clear();
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

/// Write a committed snapshot image into a latched section region.
///
/// The commit copies at most `cap` bytes into the region the matching
/// `SNAPSHOT_BEGIN` latched, so the write stays within the section it targets.
/// Stamp a channel's reuse generation into its first bytes via the cached
/// fast-path pointer.
#[inline(never)]
fn stamp_channel(ptr: *mut u8, generation: u64) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the marked channel is live and at least one byte wide.
    unsafe {
        std::ptr::copy_nonoverlapping(generation.to_be_bytes().as_ptr(), ptr, 8);
    }
}

#[inline(never)]
fn write_snapshot(target: SnapTarget, body: &[u8]) {
    if target.base.is_null() {
        return;
    }
    let n = body.len().min(target.cap);
    // SAFETY: the latched region is `cap` bytes and still owned by the session.
    unsafe {
        std::ptr::copy_nonoverlapping(body.as_ptr(), target.base, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::FieldType;
    use crate::util::ByteWriter;
    use crate::wire;

    fn stream(msgs: &[(u8, &[u8])]) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.bytes(&wire::MAGIC).u8(wire::VERSION).u8(0).u16(msgs.len() as u16);
        for (ty, payload) in msgs {
            w.u8(*ty).u8(0).u32(payload.len() as u32).bytes(payload);
        }
        w.into_vec()
    }

    fn session_open() -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u32(0x1000).u32(0);
        w.into_vec()
    }

    fn template_def(tid: u16) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u16(tid).u16(1); // template id, schema id
        w.u16(1); // field count
        w.u16(1).u8(FieldType::U32.code()).u16(0);
        w.into_vec()
    }

    fn data_record(tid: u16, flow_id: u32, octets: u64) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u16(tid).u32(flow_id).u32(0x0a00_0001).u32(0x0a00_0002).u16(1).u16(2).u8(6);
        w.u64(octets).u64(1);
        w.into_vec()
    }

    #[test]
    fn full_happy_path() {
        let data = stream(&[
            (MsgType::SessionOpen as u8, &session_open()),
            (MsgType::TemplateDef as u8, &template_def(256)),
            (MsgType::DataRecord as u8, &data_record(256, 1, 100)),
            (MsgType::DataRecord as u8, &data_record(256, 1, 50)),
        ]);
        let mut s = Session::new();
        s.process_stream(&data).unwrap();
        assert_eq!(s.metrics().records_decoded, 2);
        assert_eq!(s.flow_count(), 1);
        s.teardown();
    }

    #[test]
    fn data_before_open_is_error_but_recoverable() {
        let data = stream(&[(MsgType::DataRecord as u8, &data_record(1, 1, 1))]);
        let mut s = Session::new();
        // The stream as a whole succeeds (error is recoverable), but the record
        // is counted as an error and no flow is created.
        s.process_stream(&data).unwrap();
        assert_eq!(s.metrics().decode_errors, 1);
        assert_eq!(s.flow_count(), 0);
    }
}
