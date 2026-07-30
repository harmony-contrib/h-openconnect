const LOGS_SOURCE: &str = include_str!("../src/view/pages/logs.rs");

#[test]
fn logs_use_arkit_rsx_virtual_rows() {
    assert!(LOGS_SOURCE.contains("fn VirtualLogList("));
    assert!(LOGS_SOURCE.contains("fn VirtualLogArchiveList("));
    assert_eq!(
        LOGS_SOURCE
            .matches("use_virtual_node_adapter_items_keyed(VirtualKind::List, item_keys")
            .count(),
        2,
    );
    assert!(!LOGS_SOURCE.contains("use_virtual_node_adapter_rsx_items_keyed"));
    assert_eq!(
        LOGS_SOURCE
            .matches("use_layout_frame_node(move |host_node, _frame|")
            .count(),
        2,
    );
    assert!(LOGS_SOURCE.contains("fn VirtualLogRowView("));
    assert!(LOGS_SOURCE.contains("fn VirtualLogArchiveRowView("));
    assert!(LOGS_SOURCE.contains("fn VirtualLogArchiveAction("));
    assert!(LOGS_SOURCE.contains("onclick: move |_| on_open.call(open_item.clone())"));
    assert!(LOGS_SOURCE.contains("list_cached_count: 18_i32"));
    assert!(LOGS_SOURCE.contains("list_cached_count: 12_i32"));
    assert!(!LOGS_SOURCE.contains("NodeBuilder::new"));
    assert!(!LOGS_SOURCE.contains("ArkUINode"));
    assert!(!LOGS_SOURCE.contains("VirtualLogRenderState"));
    assert!(!LOGS_SOURCE.contains("VirtualLogArchiveRenderState"));
}

#[test]
fn rsx_archive_actions_keep_busy_and_active_guards() {
    assert!(LOGS_SOURCE.contains("enabled: !disabled"));
    assert!(LOGS_SOURCE.contains("opacity: if disabled { 0.55 } else { 1.0 }"));
    assert!(LOGS_SOURCE.contains("if !disabled"));
    assert!(LOGS_SOURCE.contains("disabled: item.export_disabled"));
    assert!(LOGS_SOURCE.contains("disabled: item.delete_disabled"));
}
