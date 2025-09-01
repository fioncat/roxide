use std::time::Duration;

/// Show elapsed time.
pub fn format_elapsed(d: Duration) -> String {
    let elapsed_time = d.as_secs_f64();

    if elapsed_time >= 3600.0 {
        let hours = elapsed_time / 3600.0;
        format!("{hours:.2}h")
    } else if elapsed_time >= 60.0 {
        let minutes = elapsed_time / 60.0;
        format!("{minutes:.2}min")
    } else if elapsed_time >= 1.0 {
        format!("{elapsed_time:.2}s")
    } else {
        let milliseconds = elapsed_time * 1000.0;
        format!("{milliseconds:.2}ms")
    }
}
