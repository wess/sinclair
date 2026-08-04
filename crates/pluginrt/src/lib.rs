//! Plugin runtime — a `wasmtime` component-model host for WASM plugins (and,
//! later, a warm-process native-tier adapter). Owns the engine, the WIT world
//! bindings, capability-gated linking, and one resident instance per plugin.
//! Defines the [`AppHost`] trait the app implements, keeping `wasmtime` out of
//! the `app` crate's own dependency surface.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

/// The generated host/guest bindings for `wit/plugin.wit`.
mod bindings {
    wasmtime::component::bindgen!({
        world: "plugin",
        path: "wit",
    });
}

use bindings::prompt::plugin::types;
/// Wire types shared with the app (mirrors the WIT `types` interface).
pub use bindings::prompt::plugin::types::{
    CommandTarget, ExecOutput, ExecRequest, HttpRequest, HttpResponse, LogLevel,
};

/// The host operations a plugin can call, implemented by the app. Each method
/// mirrors a WIT `host-*` interface function; a gated one is only reachable by a
/// plugin that declared (and was granted) the matching capability, because the
/// host links that interface only then.
pub trait AppHost: Send {
    fn log(&mut self, level: LogLevel, message: String);
    fn storage_get(&mut self, key: String) -> Option<String>;
    fn storage_set(&mut self, key: String, value: String);
    fn run_command(&mut self, text: String, target: CommandTarget) -> Result<(), String>;
    fn send_input(&mut self, bytes: Vec<u8>) -> Result<(), String>;
    fn read_screen(&mut self, lines: u32) -> Result<String, String>;
    fn selection(&mut self) -> Option<String>;
    fn fetch(&mut self, request: HttpRequest) -> Result<HttpResponse, String>;
    fn read_file(&mut self, path: String) -> Result<Vec<u8>, String>;
    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), String>;
    fn clipboard_read(&mut self) -> Result<String, String>;
    fn clipboard_write(&mut self, text: String) -> Result<(), String>;
    fn notify(&mut self, title: String, body: String);
    fn exec(&mut self, request: ExecRequest) -> Result<ExecOutput, String>;
}

/// Ceiling on one guest's linear memory. Fuel bounds how long a plugin can run
/// but says nothing about how much it can allocate, so without this a plugin
/// that grows its memory in a loop takes the whole terminal down with it — the
/// allocation failure lands in the host process, not the sandbox. Generous
/// enough for the JS SDK, which embeds an engine, and far short of trouble.
const MAX_MEMORY: usize = 64 * 1024 * 1024;

/// Store data for one plugin instance: the app host it delegates to, plus a
/// locked-down WASI context (the baseline system interface the guest's `std`
/// needs — no preopened dirs, no sockets, no env; real privileged operations go
/// through the gated `host-*` interfaces, not WASI).
struct State {
    host: Box<dyn AppHost>,
    wasi: WasiCtx,
    table: ResourceTable,
    /// Enforced by the store, which asks before every growth: a refused growth
    /// is an ordinary allocation failure inside the guest, so the plugin fails
    /// and the app carries on.
    limits: StoreLimits,
}

impl WasiView for State {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

impl types::Host for State {}

impl bindings::prompt::plugin::host_core::Host for State {
    fn log(&mut self, level: LogLevel, message: String) {
        self.host.log(level, message)
    }
    fn storage_get(&mut self, key: String) -> Option<String> {
        self.host.storage_get(key)
    }
    fn storage_set(&mut self, key: String, value: String) {
        self.host.storage_set(key, value)
    }
}

impl bindings::prompt::plugin::host_commands::Host for State {
    fn run_command(&mut self, text: String, target: CommandTarget) -> Result<(), String> {
        self.host.run_command(text, target)
    }
    fn send_input(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.host.send_input(bytes)
    }
}

impl bindings::prompt::plugin::host_screen::Host for State {
    fn read_screen(&mut self, lines: u32) -> Result<String, String> {
        self.host.read_screen(lines)
    }
    fn selection(&mut self) -> Option<String> {
        self.host.selection()
    }
}

impl bindings::prompt::plugin::host_net::Host for State {
    fn fetch(&mut self, request: HttpRequest) -> Result<HttpResponse, String> {
        self.host.fetch(request)
    }
}

impl bindings::prompt::plugin::host_fs::Host for State {
    fn read_file(&mut self, path: String) -> Result<Vec<u8>, String> {
        self.host.read_file(path)
    }
    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), String> {
        self.host.write_file(path, data)
    }
}

impl bindings::prompt::plugin::host_clipboard::Host for State {
    fn read(&mut self) -> Result<String, String> {
        self.host.clipboard_read()
    }
    fn write(&mut self, text: String) -> Result<(), String> {
        self.host.clipboard_write(text)
    }
}

impl bindings::prompt::plugin::host_notify::Host for State {
    fn notify(&mut self, title: String, body: String) {
        self.host.notify(title, body)
    }
}

impl bindings::prompt::plugin::host_process::Host for State {
    fn exec(&mut self, request: ExecRequest) -> Result<ExecOutput, String> {
        self.host.exec(request)
    }
}

/// Fuel granted per guest call. Roughly one unit per wasm instruction, so this
/// allows a lot of real work but bounds a runaway loop — a guest that exhausts
/// it traps instead of freezing the caller (the analogue of the process
/// runtime's SIGKILL, but cooperative and precise).
const FUEL_PER_CALL: u64 = 5_000_000_000;

/// Build the shared component-model engine with fuel metering on.
pub fn engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    Engine::new(&config)
}

/// A resident WASM plugin instance: a `Store` plus the instantiated component.
pub struct PluginInstance {
    store: Store<State>,
    world: bindings::Plugin,
    /// Fuel granted per call (default [`FUEL_PER_CALL`]).
    fuel: u64,
}

impl PluginInstance {
    /// Instantiate `wasm` with only the host interfaces its `capabilities` grant
    /// linked. A guest that imports an ungranted interface fails here — that is
    /// the enforced capability boundary. Runs the guest's `init` once.
    pub fn new(
        engine: &Engine,
        wasm: &[u8],
        capabilities: &[String],
        host: Box<dyn AppHost>,
    ) -> Result<Self> {
        let component = Component::new(engine, wasm).context("load plugin component")?;
        let caps: HashSet<&str> = capabilities.iter().map(String::as_str).collect();
        let mut linker: Linker<State> = Linker::new(engine);

        // Baseline WASI (clocks, random, io) so the guest's std initializes. The
        // context is empty — no filesystem, sockets, or env are exposed.
        wasmtime_wasi::add_to_linker_sync(&mut linker)?;

        // Always linked: log + per-plugin storage.
        bindings::prompt::plugin::host_core::add_to_linker(&mut linker, |s: &mut State| s)?;
        if caps.contains("commands") {
            bindings::prompt::plugin::host_commands::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("screen") {
            bindings::prompt::plugin::host_screen::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("network") {
            bindings::prompt::plugin::host_net::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("filesystem") {
            bindings::prompt::plugin::host_fs::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("clipboard") {
            bindings::prompt::plugin::host_clipboard::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("notify") {
            bindings::prompt::plugin::host_notify::add_to_linker(&mut linker, |s: &mut State| s)?;
        }
        if caps.contains("process") {
            bindings::prompt::plugin::host_process::add_to_linker(&mut linker, |s: &mut State| s)?;
        }

        let wasi = WasiCtxBuilder::new().build();
        let mut store = Store::new(
            engine,
            State {
                host,
                wasi,
                table: ResourceTable::new(),
                limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let world = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .context("instantiate plugin component")?;
        store.set_fuel(FUEL_PER_CALL)?;
        world
            .prompt_plugin_guest()
            .call_init(&mut store)
            .context("plugin init")?;
        Ok(Self {
            store,
            world,
            fuel: FUEL_PER_CALL,
        })
    }

    /// Override the per-call fuel budget (mainly for tests).
    pub fn set_fuel_budget(&mut self, fuel: u64) {
        self.fuel = fuel;
    }

    /// Call a tool the plugin exports. Returns the plugin's JSON result, or its
    /// error string. The outer `Result` is a host-side trap (a crashed guest).
    pub fn call_tool(&mut self, name: &str, params_json: &str) -> Result<Result<String, String>> {
        // Refill fuel per call so a bounded budget applies to each invocation and
        // an infinite loop traps rather than hanging the caller.
        self.store.set_fuel(self.fuel)?;
        self.world
            .prompt_plugin_guest()
            .call_call_tool(&mut self.store, name, params_json)
    }

    /// Render a panel: returns the plugin's UI node tree as a JSON string.
    pub fn render(&mut self, request_json: &str) -> Result<String> {
        self.store.set_fuel(self.fuel)?;
        self.world
            .prompt_plugin_guest()
            .call_render(&mut self.store, request_json)
    }

    /// Deliver a UI event (button click, input change) from a rendered panel.
    pub fn on_ui_event(&mut self, event_json: &str) -> Result<()> {
        self.store.set_fuel(self.fuel)?;
        self.world
            .prompt_plugin_guest()
            .call_on_ui_event(&mut self.store, event_json)
    }
}

/// Manages resident plugin instances by id over one shared engine, so callers
/// never touch the wasmtime `Engine` themselves (keeping `wasmtime` out of the
/// app's dependency surface).
pub struct Runtime {
    engine: Engine,
    instances: HashMap<String, PluginInstance>,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Ok(Self {
            engine: engine()?,
            instances: HashMap::new(),
        })
    }

    /// Whether `id` already has a resident instance.
    pub fn is_resident(&self, id: &str) -> bool {
        self.instances.contains_key(id)
    }

    /// Instantiate `id` from `wasm` (with `capabilities` + `host`) unless it is
    /// already resident. Idempotent.
    pub fn ensure(
        &mut self,
        id: &str,
        wasm: &[u8],
        capabilities: &[String],
        host: Box<dyn AppHost>,
    ) -> Result<()> {
        if !self.instances.contains_key(id) {
            let instance = PluginInstance::new(&self.engine, wasm, capabilities, host)?;
            self.instances.insert(id.to_string(), instance);
        }
        Ok(())
    }

    /// Call a tool on a resident plugin (call [`Runtime::ensure`] first). The
    /// inner `Result` is the plugin's own success/error; the outer is a host-side
    /// trap.
    pub fn call_tool(
        &mut self,
        id: &str,
        tool: &str,
        params_json: &str,
    ) -> Result<Result<String, String>> {
        let instance = self
            .instances
            .get_mut(id)
            .context("plugin is not instantiated")?;
        instance.call_tool(tool, params_json)
    }

    /// Render a resident plugin's panel to a node-tree JSON string.
    pub fn render(&mut self, id: &str, request_json: &str) -> Result<String> {
        self.instances
            .get_mut(id)
            .context("plugin is not instantiated")?
            .render(request_json)
    }

    /// Deliver a UI event to a resident plugin's panel.
    pub fn on_ui_event(&mut self, id: &str, event_json: &str) -> Result<()> {
        self.instances
            .get_mut(id)
            .context("plugin is not instantiated")?
            .on_ui_event(event_json)
    }

    /// Drop a resident instance (e.g. on plugin disable/reload).
    pub fn evict(&mut self, id: &str) {
        self.instances.remove(id);
    }
}

#[cfg(test)]
#[path = "../tests/pluginrt.rs"]
mod tests;
