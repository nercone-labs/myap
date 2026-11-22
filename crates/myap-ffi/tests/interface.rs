use myap_ffi::*;

fn call(core: *mut MyapCore, input: &str) -> String {
    let mut length = 0usize;
    // SAFETY: a live core, a valid input and a writable length.
    unsafe {
        let out = myap_core_handle(core, input.as_ptr(), input.len(), &mut length);
        assert!(!out.is_null());
        let text = String::from_utf8(std::slice::from_raw_parts(out, length).to_vec()).unwrap();
        myap_buffer_free(out, length);
        text
    }
}

#[test]
fn the_c_interface_round_trips() {
    let core = myap_core_new();
    let response = call(core, r#"{"type":"start","now":1,"data_dir":null,"locale":"ja-JP","utc_offset_minutes":540}"#);
    assert!(response.contains("\"snapshot\""));
    assert!(call(core, "not json").contains("\"error\""));
    assert!(call(core, "").contains("\"error\""));
    // SAFETY: null arguments are refused, not read.
    unsafe {
        let mut length = 0usize;
        assert!(myap_core_handle(std::ptr::null_mut(), std::ptr::null(), 0, &mut length).is_null());
        assert!(myap_core_handle(core, std::ptr::null(), 0, std::ptr::null_mut()).is_null());
        let bad = [0xffu8, 0xfe];
        let out = myap_core_handle(core, bad.as_ptr(), bad.len(), &mut length);
        let text = String::from_utf8(std::slice::from_raw_parts(out, length).to_vec()).unwrap();
        assert!(text.contains("not UTF-8"));
        myap_buffer_free(out, length);
        myap_core_free(core);
        myap_core_free(std::ptr::null_mut());
    }
}

#[test]
fn graph_builds_report_errors() {
    let input = r#"{"input":"/nonexistent/x.osm.pbf","format":"pbf","output":"/nonexistent/x.myapgraph"}"#;
    let mut length = 0usize;
    // SAFETY: valid input and length.
    unsafe {
        let out = myap_build_graph(input.as_ptr(), input.len(), &mut length);
        let text = String::from_utf8(std::slice::from_raw_parts(out, length).to_vec()).unwrap();
        myap_buffer_free(out, length);
        assert!(text.starts_with("{\"Err\""), "{text}");
    }
}
