//! Encoding a frame stream to MP4/MOV/WebM by piping raw frames to ffmpeg.
//!
//! We feed rawvideo RGBA on ffmpeg's stdin at a fixed frame rate and let it do
//! the codec work. ffmpeg's stderr is drained on a thread so its progress output
//! can never fill the pipe and stall the encode.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::options::Format;
use crate::raster::Raster;
use crate::replay::Renderer;
use crate::Error;

pub fn encode<R: Raster>(
  renderer: &mut Renderer<R>,
  out: &Path,
  format: Format,
) -> Result<(), Error> {
  let (w, h) = renderer.dimensions();
  let fps = renderer.fps();

  let mut cmd = Command::new("ffmpeg");
  cmd
    .arg("-y")
    .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
    .args(["-s", &format!("{w}x{h}")])
    .args(["-r", &fps.to_string()])
    .args(["-i", "-"]);
  match format {
    Format::Webm => {
      cmd.args([
        "-c:v",
        "libvpx-vp9",
        "-pix_fmt",
        "yuv420p",
        "-b:v",
        "0",
        "-crf",
        "30",
      ]);
    }
    // H.264 needs even dimensions and yuv420p for broad playback.
    _ => {
      cmd
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"])
        .args(["-preset", "medium", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"]);
    }
  }
  cmd
    .arg(out)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

  let mut child = match cmd.spawn() {
    Ok(child) => child,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::FfmpegMissing),
    Err(e) => return Err(Error::Io(e)),
  };

  let mut stdin = child.stdin.take().expect("stdin piped");
  let stderr = child.stderr.take().expect("stderr piped");
  let drain = std::thread::spawn(move || {
    let mut log = String::new();
    let mut stderr = stderr;
    let _ = stderr.read_to_string(&mut log);
    log
  });

  let mut write_err: Option<std::io::Error> = None;
  renderer.run(|img| {
    if write_err.is_some() {
      return;
    }
    if let Err(e) = stdin.write_all(&img.data) {
      write_err = Some(e);
    }
  });
  drop(stdin);

  let status = child.wait().map_err(Error::Io)?;
  let log = drain.join().unwrap_or_default();

  if !status.success() {
    return Err(Error::Ffmpeg(log.trim().to_string()));
  }
  // A write failure with a successful exit is unusual; surface it anyway.
  if let Some(e) = write_err {
    return Err(Error::Io(e));
  }
  Ok(())
}
