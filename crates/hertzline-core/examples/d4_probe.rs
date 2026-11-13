//! Print D4 sensor probe JSON (HZ-007).

use hertzline_core::sensors::probe_sensors;

fn main() {
    let report = probe_sensors();
    println!("{}", serde_json::to_string_pretty(&report).expect("json"));
}
