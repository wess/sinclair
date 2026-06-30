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

    /// Called when a pane is about to be removed: force-remove its on-the-fly
    /// container (if it was an ephemeral run-fresh tab) and drop any attach
    /// mapping pointing at it. Runs `docker rm -f <name>` detached, so a closing
    /// tab does not block on the engine.
    pub(crate) fn on_pane_closed(&mut self, pane: PaneId) {
        if let Some(name) = self.kill_on_close.remove(&pane) {
            if let Some(engine) = self.container_engine() {
                let _ = std::process::Command::new(engine.binary())
                    .args(["rm", "-f", &name])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
        self.container_tabs.retain(|_, p| *p != pane);
    }

    /// Re-run `docker ps` and cache the result. Blocking I/O, so only call this
    /// on explicit user action (panel open / refresh), never during render.
    pub(crate) fn refresh_containers(&mut self) {
        let Some(engine) = self.container_engine() else {
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
        if let Some(&pid) = self.container_tabs.get(&running.id) {
            if self.panes.contains_key(&pid) {
                self.focuspane(pid, window, cx);
                return;
            }
            self.container_tabs.remove(&running.id);
        }
        let Some(engine) = self.container_engine() else {
            eprintln!("prompt: no container engine available (install Docker or Podman)");
            return;
        };
        let argv = container::attach_argv(engine, &running.id);
        let Some(id) = self.spawn_tab_argv(argv, window, cx) else {
            return;
        };
        self.tabs.new_tab(id);
        let label = if running.name.is_empty() {
            running.id.clone()
        } else {
            running.name.clone()
        };
        let index = self.tabs.active_index();
        self.rename_tab(index, &label, cx);
        self.container_tabs.insert(running.id.clone(), id);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn a pane running `argv` directly (no shell wrapper), inheriting the
    /// focused pane's cwd. Shared by container run/attach paths.
    pub(crate) fn spawn_tab_argv(
        &mut self,
        argv: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let inherit = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.view.read(cx).cwd())
            .and_then(|osc| session::cwdpath(&osc));
        let mut options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        let cwd = options.spawn.cwd.clone();
        options.spawn = pty::SpawnOptions::command(argv);
        options.spawn.cwd = cwd;
        self.spawn(options, window, cx)
    }
}
