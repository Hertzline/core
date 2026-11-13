fn main() {
    let r = hertzline_core::frame_capture::probe_frame_capture();
    println!("{}", serde_json::to_string_pretty(&r).unwrap());
}
