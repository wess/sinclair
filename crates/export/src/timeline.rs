//! Turning recorded event times into playback times.
//!
//! A raw recording keeps real wall-clock gaps: a session with a four-minute
//! pause makes four minutes of frozen video. `adjust` rewrites the timeline by
//! capping idle gaps and applying a speed multiplier, so the clip stays
//! watchable while preserving the relative rhythm of activity.

/// Rewrite monotonically non-decreasing event `times` into playback times.
///
/// The first event maps to `0.0`; each later event advances by its gap from the
/// previous event, first clamped to `idle_cap` seconds (when set) and then
/// divided by `speed`. The result has the same length as the input.
pub fn adjust(times: &[f64], speed: f64, idle_cap: Option<f64>) -> Vec<f64> {
  let speed = if speed > 1e-6 { speed } else { 1e-6 };
  let mut out = Vec::with_capacity(times.len());
  let mut acc = 0.0;
  let mut prev = times.first().copied().unwrap_or(0.0);
  for (i, &t) in times.iter().enumerate() {
    if i > 0 {
      let gap = (t - prev).max(0.0);
      let capped = match idle_cap {
        Some(c) => gap.min(c.max(0.0)),
        None => gap,
      };
      acc += capped / speed;
      prev = t;
    }
    out.push(acc);
  }
  out
}

#[cfg(test)]
#[path = "../tests/timeline.rs"]
mod tests;
