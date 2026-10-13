//! Prints every gilrs event with a timestamp. Use it to check that the controller is
//! seen at all, and (bundled via `scripts/bundle.sh probe`) that events still arrive
//! while another app is focused. Logs to stdout and /tmp/controller-mouse-probe.log.

use std::io::Write;
use std::time::Instant;

fn main() {
    let mut gilrs = gilrs::Gilrs::new().expect("gilrs init");
    let mut log = std::fs::File::create("/tmp/controller-mouse-probe.log").expect("log file");
    let start = Instant::now();
    let mut out = |line: String| {
        println!("{line}");
        let _ = writeln!(log, "{line}");
    };
    for (id, gp) in gilrs.gamepads() {
        out(format!("connected at start: {id} {} ({:?})", gp.name(), gp.mapping_source()));
    }
    out("waiting for events… (Ctrl+C to quit)".into());
    loop {
        if let Some(ev) = gilrs.next_event_blocking(None) {
            out(format!("{:>8.3}s {} {:?}", start.elapsed().as_secs_f32(), ev.id, ev.event));
        }
    }
}
