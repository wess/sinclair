//! Containers sidebar panel backing logic: refreshing the running-container
//! list (off the render path) and attaching a tab to a container.

use super::*;
use gpui::prelude::*;
use std::process::Command;

impl WorkspaceView {
    /// The resolved container engine per config (`docker`/`podman`/auto), or
    /// `None` when none is installed.
    pub(crate) fn container_engine(&self) -> Option<container::Engine> {
        container::Engine::resolve(self.opts.container_engine.as_deref())
    }

    /// Called when an item is about to be removed: force-remove its on-the-fly
    /// container (if it was an ephemeral run-fresh tab) and drop any attach
    /// mapping pointing at it. Runs `docker rm -f <name>` detached, so a closing
    /// tab does not block on the engine.
    pub(crate) fn on_item_closed(&mut self, item: ItemId) {
        if let Some(name) = self.kill_on_close.remove(&item) {
            if let Some(engine) = self.container_engine() {
                let _ = std::process::Command::new(engine.binary())
                    .args(["rm", "-f", &name])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
        self.container_tabs.retain(|_, p| *p != item);
    }

    /// Re-run `docker ps` and cache the result (plus the resolved engine, which
    /// the panel renders from). Blocking I/O, so only call this on explicit
    /// user action (panel open / refresh), never during render.
    pub(crate) fn refresh_containers(&mut self) {
        let engine = self.container_engine();
        self.engine = Some(engine);
        let Some(engine) = engine else {
            self.containers.clear();
            return;
        };
        let argv = container::ps_argv(engine);
        match Command::new(&argv[0]).args(&argv[1..]).output() {
            Ok(out) if out.status.success() => {
                self.containers = container::parse_ps(&String::from_utf8_lossy(&out.stdout));
            }
            _ => self.containers.clear(),
        }
    }

    /// Attach a tab to `running`: focus its existing tab when one is already
    /// open, otherwise spawn a new tab exec-ing an interactive shell into it.
    pub(crate) fn attach_container(
        &mut self,
        running: &container::Running,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(&iid) = self.container_tabs.get(&running.id) {
            if self.items.borrow().contains_key(&iid) {
                self.activate_item(iid, window, cx);
                return;
            }
            self.container_tabs.remove(&running.id);
        }
        let Some(engine) = self.container_engine() else {
            eprintln!("sinclair: no container engine available (install Docker or Podman)");
            return;
        };
        let argv = container::attach_argv(engine, &running.id);
        let Some(id) = self.spawn_tab_argv(argv, window, cx) else {
            return;
        };
        self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
        let label = if running.name.is_empty() {
            running.id.clone()
        } else {
            running.name.clone()
        };
        self.rename_item(id, &label, cx);
        self.container_tabs.insert(running.id.clone(), id);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn an item running `argv` directly (no shell wrapper), inheriting the
    /// focused item's cwd. Shared by container run/attach paths.
    pub(crate) fn spawn_tab_argv(
        &mut self,
        argv: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        let inherit = self.focused_cwd_path(cx);
        let mut options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        let cwd = options.spawn.cwd.clone();
        options.spawn = pty::SpawnOptions::command(argv);
        options.spawn.cwd = cwd;
        self.spawn(options, window, cx)
    }
}
