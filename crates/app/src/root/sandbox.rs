//! Host side of the shared project sandbox.
//!
//! One container serves a whole project: the human's panes and every agent on
//! a team all `exec` into it, so there is a single filesystem and a single
//! toolchain rather than one per participant. That makes three things this
//! module's job:
//!
//! - **Resolve once.** Bringing the sandbox up can build an image, so it runs
//!   on the background executor and the result is cached on the window.
//! - **Count panes, not tabs.** The container outlives any single pane. It is
//!   retired when the last one leaves, and only if Sinclair created it.
//! - **Never touch what it did not create.** A container VS Code built very
//!   likely has the user's editor attached to it.

use super::*;
use gpui::prelude::*;

/// The active workspace window's sandbox, for surfaces that build a launch
/// command from outside a window (the New Agent picker). `None` when no
/// sandbox is up, which is a host launch.
pub(crate) fn active_ref(app: &mut App) -> Option<crate::relay::SandboxRef> {
    let handle = crate::mcpbridge::active_workspace(app)?;
    handle
        .update(app, |ws, _window, _cx| ws.sandbox_ref())
        .ok()
        .flatten()
}

impl WorkspaceView {
    /// The project the sandbox serves: the repository containing the focused
    /// pane, else that pane's own working directory.
    ///
    /// The repository root, not the pane's cwd, because a pane sitting three
    /// directories down would otherwise get its own sandbox with only that
    /// subtree mounted — and the team would be split across containers.
    pub(crate) fn sandbox_project(&self, cx: &App) -> Option<String> {
        let cwd = self.focused_cwd_path(cx)?;
        let dir = crate::sandbox::repo_root(&cwd).unwrap_or(cwd);
        let path = dir.to_string_lossy().trim_end_matches('/').to_string();
        // A sandbox identity-mounts what it is given. Refusing `$HOME` and `/`
        // is the difference between mounting a project and handing an agent
        // with its prompts bypassed the user's entire machine.
        if path.is_empty() || crate::sandbox::too_broad_to_mount(&path) {
            return None;
        }
        Some(path)
    }

    /// This window's sandbox as launch flags, or `None` for a host launch.
    pub(crate) fn sandbox_ref(&self) -> Option<crate::relay::SandboxRef> {
        crate::relay::sandbox_ref(self.sandbox.as_ref().map(|r| &r.sandbox))
    }

    /// One line describing the sandbox for menus and the sidebar.
    pub(crate) fn sandbox_summary(&self) -> String {
        if let Some(status) = &self.sandbox_status {
            return status.clone();
        }
        match &self.sandbox {
            Some(ready) => {
                let who = if ready.adopted { ready.owner.label() } else { "Sinclair" };
                format!(
                    "Running \u{00b7} {} pane{} \u{00b7} {who}",
                    self.sandbox_panes.len(),
                    if self.sandbox_panes.len() == 1 { "" } else { "s" }
                )
            }
            None if !self.opts.sandbox_enabled => "Off for this project".to_string(),
            None if self.container_engine().is_none() => {
                "No container engine (install Docker or Podman)".to_string()
            }
            None => "Not started".to_string(),
        }
    }

    /// Resolve settings, the project, and any `devcontainer.json` into a spec.
    /// Reads the config file from disk, so it is called off the render path.
    fn sandbox_spec(&self, project: &str) -> crate::sandbox::Spec {
        let (dc, note) = if self.opts.sandbox_devcontainer {
            crate::sandbox::read_devcontainer(project)
        } else {
            (None, None)
        };
        let engine = self.container_engine().unwrap_or(container::Engine::Docker);
        let relay_dir = crate::relay::home().join("sandbox");
        let relay_dir = relay_dir.to_string_lossy().into_owned();
        let _ = std::fs::create_dir_all(&relay_dir);
        let mut spec = crate::sandbox::build(
            &self.opts,
            &crate::sandbox::Env {
                engine,
                project,
                devcontainer: dc.as_ref(),
                host_user: crate::sandbox::owner_of(project),
                relay_dir: Some(&relay_dir),
                default_agent: &self.opts.relay_default_agent,
            },
        );
        if let Some(note) = note {
            spec.notes.push(note);
        }
        spec
    }

    /// Bring the sandbox up (if it is not already) and hand it to `then`.
    ///
    /// The engine calls block — a first run pulls a base image and installs an
    /// agent CLI — so they happen on the background executor and `then` runs
    /// back on the window once the container is ready.
    pub(crate) fn with_sandbox(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, container::Sandbox, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if let Some(ready) = &self.sandbox {
            let sandbox = ready.sandbox.clone();
            then(self, sandbox, window, cx);
            return;
        }
        let Some(handle) = window.window_handle().downcast::<Self>() else {
            return;
        };
        if self.container_engine().is_none() {
            self.sandbox_fail("No container engine found. Install Docker or Podman.", cx);
            return;
        }
        let Some(project) = self.sandbox_project(cx) else {
            self.sandbox_fail(
                "No project here to sandbox. Open a pane inside a repository — a sandbox \
                 mounts what it is given, and your home directory is too much to hand an \
                 agent.",
                cx,
            );
            return;
        };
        if self.sandbox_busy {
            return;
        }
        self.sandbox_busy = true;
        self.sandbox_status = Some(crate::sandbox::Stage::Looking.label().to_string());
        cx.notify();

        let spec = self.sandbox_spec(&project);
        let executor = cx.background_executor().clone();

        // A first run pulls a base image and installs an agent CLI, which is
        // minutes, not milliseconds. The worker records what it is doing and a
        // foreground ticker repaints the status line, so the wait is legible
        // instead of a frozen menu item.
        let stage = std::sync::Arc::new(std::sync::Mutex::new(crate::sandbox::Stage::Looking));
        let ticker = stage.clone();
        let ticks = executor.clone();
        cx.spawn(async move |_this, cx| loop {
            ticks.timer(std::time::Duration::from_millis(200)).await;
            let now = *ticker.lock().unwrap_or_else(|e| e.into_inner());
            let running = handle
                .update(cx, |view, _window, cx| {
                    if view.sandbox_busy {
                        view.sandbox_status = Some(now.label().to_string());
                        cx.notify();
                    }
                    view.sandbox_busy
                })
                .unwrap_or(false);
            if !running {
                break;
            }
        })
        .detach();

        cx.spawn(async move |_this, cx| {
            let result = executor
                .spawn(async move {
                    crate::sandbox::ensure(&spec, &move |s| {
                        *stage.lock().unwrap_or_else(|e| e.into_inner()) = s;
                    })
                })
                .await;
            let _ = handle.update(cx, |view, window, cx| {
                view.sandbox_busy = false;
                match result {
                    Ok(ready) => {
                        for note in &ready.notes {
                            eprintln!("sinclair: sandbox: {note}");
                        }
                        view.sandbox_notes = ready.notes.clone();
                        view.sandbox_status = None;
                        let sandbox = ready.sandbox.clone();
                        view.sandbox = Some(ready);
                        view.setmenus(cx);
                        then(view, sandbox, window, cx);
                    }
                    Err(e) => view.sandbox_fail(&e, cx),
                }
            });
        })
        .detach();
    }

    /// Record a failure where the user can see it: the menu status line, the
    /// sidebar, and stderr.
    fn sandbox_fail(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("sinclair: sandbox: {message}");
        self.sandbox_busy = false;
        self.sandbox_status = Some(message.to_string());
        self.setmenus(cx);
        cx.notify();
    }

    /// Open an interactive login shell in the sandbox, in a new tab.
    pub(crate) fn open_sandbox_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_sandbox(window, cx, |this, sandbox, window, cx| {
            let cwd = this.sandbox_project(cx);
            let argv = sandbox.shell_argv(cwd.as_deref());
            let Some(id) = this.spawn_tab_argv(argv, window, cx) else {
                return;
            };
            this.group.update(cx, |g, cx| g.add_to_focused(id, cx));
            this.rename_item(id, "Sandbox", cx);
            this.sandbox_panes.insert(id);
            // The status line counts attached panes, so it has to be rebuilt
            // *after* this one joins — `with_sandbox` rebuilds before handing
            // the sandbox over, and takes a shortcut that skips it entirely
            // when the container is already up.
            this.setmenus(cx);
            this.focusactive(window, cx);
            cx.notify();
        });
    }

    /// Bring the container up without opening anything.
    pub(crate) fn sandbox_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_sandbox(window, cx, |this, _sandbox, _window, cx| {
            this.refresh_containers();
            cx.notify();
        });
    }

    /// Stop the container. Panes inside it end with it, which is why the menu
    /// item says so.
    pub(crate) fn sandbox_stop(&mut self, cx: &mut Context<Self>) {
        let Some(ready) = self.sandbox.take() else {
            return;
        };
        if !ready.owner.may_remove() {
            self.sandbox = Some(ready);
            self.sandbox_fail(
                "This container was not created by Sinclair, so it is left alone.",
                cx,
            );
            return;
        }
        let sandbox = ready.sandbox.clone();
        let owner = ready.owner;
        cx.background_executor()
            .spawn(async move {
                crate::sandbox::ensure::stop(&sandbox, owner);
            })
            .detach();
        self.sandbox_panes.clear();
        self.sandbox_status = None;
        self.setmenus(cx);
        cx.notify();
    }

    /// Remove the container so the next open rebuilds it from the current
    /// settings. Anything written outside the mounted project is lost.
    pub(crate) fn sandbox_rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ready) = self.sandbox.take() {
            if !ready.owner.may_remove() {
                self.sandbox = Some(ready);
                self.sandbox_fail(
                    "This container was not created by Sinclair, so it is left alone.",
                    cx,
                );
                return;
            }
            let sandbox = ready.sandbox.clone();
            let owner = ready.owner;
            // Blocking: the next step must not race the removal.
            crate::sandbox::ensure::remove(&sandbox, owner);
        }
        self.sandbox_panes.clear();
        self.sandbox_start(window, cx);
    }

    /// Flip `sandbox-enabled` and write it back to the settings file, so the
    /// menu toggle is a real setting rather than a session-only switch.
    pub(crate) fn toggle_sandbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on = !self.opts.sandbox_enabled;
        self.opts.sandbox_enabled = on;
        crate::confwrite::upsert("sandbox-enabled", if on { "true" } else { "false" });
        if !on {
            self.sandbox_stop(cx);
        } else {
            self.sandbox_start(window, cx);
        }
        self.setmenus(cx);
        cx.notify();
    }

    /// A pane using the sandbox has closed. The container outlives individual
    /// panes, so this only retires it when the last one leaves *and* the user
    /// asked for that — and never when Sinclair did not create it.
    pub(crate) fn sandbox_detach(&mut self, item: ItemId, cx: &mut Context<Self>) {
        if !self.sandbox_panes.remove(&item) {
            return;
        }
        self.setmenus(cx);
        if !self.sandbox_panes.is_empty() {
            return;
        }
        if self.opts.sandbox_persist {
            return;
        }
        let Some(ready) = &self.sandbox else {
            return;
        };
        if !ready.owner.may_remove() {
            return;
        }
        let sandbox = ready.sandbox.clone();
        let owner = ready.owner;
        std::thread::spawn(move || {
            crate::sandbox::ensure::stop(&sandbox, owner);
        });
        self.sandbox = None;
    }
}
