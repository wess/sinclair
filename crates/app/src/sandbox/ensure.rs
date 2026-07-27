//! Bringing a sandbox up: the blocking engine calls, in order.
//!
//! Resolution always looks before it creates. A project may already have a
//! container — one Sinclair started earlier, or one VS Code's Dev Containers
//! extension built — and starting a second beside it would split the team
//! across two filesystems. Whatever is found is entered; only a project with
//! nothing running gets a fresh container.
//!
//! Every call here blocks on a subprocess, so this runs on the background
//! executor, never on the render path.

use super::spec::Spec;
use container::{adopt, Owner, Sandbox, State};
use std::io::Write;
use std::process::{Command, Stdio};

/// What the host is doing, reported as it happens so the UI can say something
/// truthful during a first run that pulls a base image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Looking,
    Building,
    Creating,
    Starting,
    Ready,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Looking => "Looking for the sandbox\u{2026}",
            Self::Building => "Building the sandbox image\u{2026}",
            Self::Creating => "Creating the sandbox\u{2026}",
            Self::Starting => "Starting the sandbox\u{2026}",
            Self::Ready => "Sandbox ready",
        }
    }
}

/// A sandbox that is up and can be exec'd into.
#[derive(Debug, Clone)]
pub struct Ready {
    pub sandbox: Sandbox,
    /// Who created the container. Sinclair only ever stops or removes its own:
    /// a container VS Code built very likely has the user's editor attached.
    pub owner: Owner,
    /// True when an existing container was entered rather than created.
    pub adopted: bool,
    pub notes: Vec<String>,
}

/// Bring `spec` up, reporting progress through `report`.
pub fn ensure(spec: &Spec, report: &dyn Fn(Stage)) -> Result<Ready, String> {
    let engine = spec.sandbox.engine;
    let mut notes = spec.notes.clone();

    report(Stage::Looking);
    let found = run(&adopt::find_argv(engine, &spec.project))
        .map(|out| adopt::parse_found(&out))
        .unwrap_or_default();

    // Someone else's container for this project, already up: enter it rather
    // than building a parallel one.
    if let Some(f) = adopt::best(&found).filter(|f| f.owner == Owner::Foreign && f.state.is_running())
    {
        notes.push(format!(
            "Attached to `{}`, a container Sinclair did not create. It will not be stopped or \
             removed from here.",
            f.name
        ));
        let mut sandbox = spec.sandbox.clone();
        sandbox.name = f.name.clone();
        report(Stage::Ready);
        return Ok(Ready {
            sandbox,
            owner: Owner::Foreign,
            adopted: true,
            notes,
        });
    }

    let sandbox = &spec.sandbox;
    match state(sandbox)? {
        State::Running => {}
        s if s.is_stopped() => {
            report(Stage::Starting);
            run(&sandbox.start_argv()).map_err(|e| format!("could not start the sandbox: {e}"))?;
        }
        _ => {
            if let Some(recipe) = &spec.recipe {
                let tag = recipe.tag();
                let built = run(&container::exists_argv(engine, &tag))
                    .map(|out| !out.trim().is_empty())
                    .unwrap_or(false);
                if !built {
                    report(Stage::Building);
                    build(recipe, engine)?;
                }
            }
            report(Stage::Creating);
            run(&sandbox.create_argv())
                .map_err(|e| format!("could not create the sandbox: {e}"))?;
        }
    }

    report(Stage::Ready);
    Ok(Ready {
        sandbox: sandbox.clone(),
        owner: Owner::Sinclair,
        adopted: false,
        notes,
    })
}

/// The container's current state, or [`State::Missing`] when the engine cannot
/// be reached at all (which surfaces as a create attempt and a real error).
fn state(sandbox: &Sandbox) -> Result<State, String> {
    Ok(container::parse_state(&run(&sandbox.state_argv()).unwrap_or_default()))
}

/// Build the recipe's image, piping the generated Dockerfile in on stdin so
/// there is no build context on disk to create or clean up.
fn build(recipe: &container::Recipe, engine: container::Engine) -> Result<(), String> {
    let argv = recipe.build_argv(engine);
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run {}: {e}", argv[0]))?;
    child
        .stdin
        .take()
        .ok_or("no stdin on the build")?
        .write_all(recipe.dockerfile().as_bytes())
        .map_err(|e| format!("could not send the Dockerfile: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("build failed: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(format!(
        "could not build the sandbox image: {}",
        last_line(&err)
    ))
}

/// Stop a sandbox, if Sinclair is allowed to. Returns whether it acted.
pub fn stop(sandbox: &Sandbox, owner: Owner) -> bool {
    owner.may_remove() && run(&sandbox.stop_argv()).is_ok()
}

/// Remove a sandbox, if Sinclair is allowed to.
pub fn remove(sandbox: &Sandbox, owner: Owner) -> bool {
    owner.may_remove() && run(&sandbox.rm_argv()).is_ok()
}

/// Run an argv to completion, returning stdout. A non-zero exit is an error
/// carrying the engine's last line of stderr, which is the part worth showing.
fn run(argv: &[String]) -> Result<String, String> {
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| format!("could not run {}: {e}", argv[0]))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(last_line(&String::from_utf8_lossy(&out.stderr)))
    }
}

/// The last non-empty line of engine output — engines print progress before
/// the message that actually explains the failure.
fn last_line(s: &str) -> String {
    s.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown error")
        .to_string()
}
