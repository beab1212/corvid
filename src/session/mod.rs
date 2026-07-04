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
use crate::codec::{Codec, CompressorState};
use crate::config::Config;
use crate::engine::{Modules, ScopeStack, SymbolTable, Vm};
use crate::error::{Error, Result};
use crate::flow::{ConnRegistry, FlowTable};
use crate::metrics::Metrics;
use crate::parser::{FrameParser, Message};
use crate::reassembly::{Reassembler, Window};
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

    reasm: HashMap<u32, Reassembler>,
    streams: HashMap<u32, StreamCtx>,

    conns: ConnRegistry,

    modules: Modules,
    symbols: SymbolTable,
    scopes: ScopeStack,
    vm_result: i64,

    compressor: CompressorState,
    channels: HashMap<u32, Vec<u8>>,
    sections: Vec<RegionPool>,

    metrics: Metrics,
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
            reasm: HashMap::new(),
            streams: HashMap::new(),
            conns: ConnRegistry::new(cfg.window_size.min(4096)),
            modules: Modules::new(),
            symbols: SymbolTable::new(),
            scopes: ScopeStack::new(),
            vm_result: 0,
            compressor,
            channels: HashMap::new(),
            sections: Vec::new(),
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
            SnapshotBegin | SnapshotCommit => Ok(()),
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

        let (gen, field_count) = {
            let tmpl = self
                .templates
                .get(hdr.template_id)
                .ok_or_else(|| Error::unresolved("data for unknown template"))?;
            (tmpl.generation, tmpl.field_count() as u16)
        };

        self.metrics.note_record();
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
        let limit = r.u32()? as usize;
        let data = r.rest();
        let out = crate::codec::compress::inflate_block(
            codec,
            data,
            limit.clamp(1, self.cfg.window_size * 16),
        )?;
        self.metrics.note_codec(data.len(), out.len());
        Ok(())
    }

    fn on_compress_config(&mut self, payload: &[u8]) -> Result<()> {
        self.require_open()?;
        let mut r = ByteReader::new(payload);
        let codec = r.u8()?;
        let limit = r.u32().unwrap_or(0) as usize;
        self.compressor.configure(codec, limit)
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
        let mut pool = RegionPool::with_dims(count, stride)?;
        let body = r.rest();
        let rows = (body.len() / stride).min(count);
        for i in 0..rows {
            pool.write_row(i, &body[i * stride..(i + 1) * stride])?;
        }
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
        self.vm_result = vm.run(&self.modules, &self.symbols, sym_id)?;
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
        self.channels.entry(id).or_insert_with(|| vec![0u8; size]);
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
        if let Some(buf) = self.channels.get_mut(&id) {
            buf.resize(size, 0);
        } else {
            self.channels.insert(id, vec![0u8; size]);
            self.metrics.live_channels += 1;
        }
        Ok(())
    }

    // --- nested records --------------------------------------------------

    fn on_nested(&mut self, payload: &[u8], depth: u32) -> Result<()> {
        self.require_open()?;
        if depth > 8 {
            return Err(Error::limit("nested record too deep"));
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
