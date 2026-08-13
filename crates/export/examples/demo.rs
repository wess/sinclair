//! Synthesize a recording and export it to every format. Manual smoke test:
//! `cargo run -p export --example demo -- <out-dir>`.

use std::io::Write;
use std::path::PathBuf;

use export::{export, Options};

fn write_cast(path: &PathBuf) {
    let mut f = std::fs::File::create(path).unwrap();
    writeln!(f, "{{\"version\":2,\"width\":40,\"height\":8}}").unwrap();
    // A little animated, colored sequence.
    let mut t = 0.0f64;
    let ev = |f: &mut std::fs::File, t: f64, s: &str| {
        writeln!(f, "[{t}, \"o\", {}]", serde_json::to_string(s).unwrap()).unwrap();
    };
    ev(
        &mut f,
        t,
        "\x1b[2J\x1b[H\x1b[1;32mprompt\x1b[0m export demo\r\n",
    );
    t += 0.3;
    ev(&mut f, t, "building ");
    for i in 0..20 {
        t += 0.08;
        let bar = "#".repeat(i + 1);
        ev(
            &mut f,
            t,
            &format!("\r\x1b[36m[{bar:<20}]\x1b[0m {}%", (i + 1) * 5),
        );
    }
    t += 0.4;
    ev(&mut f, t, "\r\n\x1b[1;33mdone.\x1b[0m\r\n");
    t += 1.0;
    ev(&mut f, t, "$ ");
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cast = dir.join("demo.cast");
    write_cast(&cast);

    let opts = Options::default();
    for name in ["demo.gif", "demo.mp4", "demo.webm", "demo.mov"] {
        let out = dir.join(name);
        match export(&cast, &out, &opts) {
            Ok(()) => {
                let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                println!("  ok   {name:10}  {size:>9} bytes");
            }
            Err(e) => println!("  FAIL {name:10}  {e}"),
        }
    }
    println!("cast: {}", cast.display());
}
