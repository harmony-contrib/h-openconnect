use super::*;

#[test]
fn new_profiles_use_strict_certificate_defaults() {
    let profile = ConnectionProfile::new_draft();
    assert!(profile.strict_certificate_trust);
    assert!(profile.block_untrusted_servers);
    assert!(!profile.allow_insecure_crypto);
}

#[test]
fn auto_reconnect_requires_an_unexpected_connected_to_terminal_edge() {
    assert!(ConnectionLifecycle::Connected.should_auto_reconnect_to(
        ConnectionLifecycle::Failed,
        false,
        false,
        true,
    ));
    assert!(ConnectionLifecycle::Connected.should_auto_reconnect_to(
        ConnectionLifecycle::Disconnected,
        false,
        false,
        true,
    ));
    assert!(!ConnectionLifecycle::Failed.should_auto_reconnect_to(
        ConnectionLifecycle::Failed,
        false,
        false,
        true,
    ));
    assert!(!ConnectionLifecycle::Connected.should_auto_reconnect_to(
        ConnectionLifecycle::Failed,
        true,
        false,
        true,
    ));
    assert!(!ConnectionLifecycle::Connected.should_auto_reconnect_to(
        ConnectionLifecycle::Failed,
        false,
        true,
        true,
    ));
}

#[test]
fn profile_validation_rejects_invalid_network_settings() {
    let mut profile = ConnectionProfile::new_draft();
    profile.name = "Corp".to_owned();
    profile.server = "vpn.example.test".to_owned();
    profile.mtu = 500;
    assert!(profile.validate().unwrap_err().contains("MTU"));
    profile.mtu = 1400;
    profile.split_tunnel_networks = "10.0.0.0/not-a-prefix".to_owned();
    assert!(profile.validate().unwrap_err().contains("split-tunnel"));
}

#[test]
fn handoff_preserves_normalized_backup_servers_and_client_identity() {
    let mut profile = ConnectionProfile::new_draft();
    profile.id = "privacy-scoped-profile".to_owned();
    profile.backup_servers =
        "backup-a.example.test, https://backup-b.example.test/group".to_owned();
    profile.client_version = "5.1.2".to_owned();
    let options = VpnOptions::from_network(&NetworkSnapshot::default(), &profile);
    assert_eq!(
        options.backup_servers,
        vec![
            "https://backup-a.example.test".to_owned(),
            "https://backup-b.example.test/group".to_owned()
        ]
    );
    assert_eq!(options.mobile_unique_id, "privacy-scoped-profile");
    assert_eq!(options.client_version, "5.1.2");
    assert!(
        options.addresses.is_empty(),
        "an empty network snapshot must not fabricate a tunnel address"
    );
}

#[test]
fn normalize_dotted_netmask_routes() {
    assert_eq!(
        normalize_route_cidr("11.0.0.0/255.0.0.0").as_deref(),
        Some("11.0.0.0/8")
    );
    assert_eq!(
        normalize_route_cidr("10.20.205.51/255.255.255.255").as_deref(),
        Some("10.20.205.51/32")
    );
    assert_eq!(
        normalize_route_cidr("0.0.0.0/0").as_deref(),
        Some("0.0.0.0/0")
    );
    assert_eq!(
        normalize_route_cidr("192.168.1.0/24").as_deref(),
        Some("192.168.1.0/24")
    );
}

#[test]
fn from_network_follows_ics_dns_and_cidr() {
    let mut profile = ConnectionProfile::new_draft();
    profile.force_global = false;
    let network = NetworkSnapshot {
        address: Some("11.36.1.2".into()),
        netmask: Some("255.255.224.0".into()),
        address_v6: None,
        netmask_v6: None,
        gateway: None,
        dns: vec!["11.11.11.11".into(), "11.11.11.12".into()],
        mtu: 1300,
        routes: vec![
            "11.0.0.0/255.0.0.0".into(),
            "10.20.205.51/255.255.255.255".into(),
        ],
        split_excludes: vec![],
        domain: Some("sslvpn.sankuai.info".into()),
        split_dns: vec!["sankuai.com".into(), "meituan.net".into()],
    };
    let options = VpnOptions::from_network(&network, &profile);
    // ics addAddress(addr, netmask bits) — 255.255.224.0 → /19
    assert_eq!(options.addresses, vec!["11.36.1.2/19".to_owned()]);
    assert!(options.routes.iter().any(|r| r == "11.0.0.0/8"));
    assert!(options.routes.iter().any(|r| r == "10.20.205.51/32"));
    // ics always adds DNS host routes.
    assert!(options.routes.iter().any(|r| r == "11.11.11.11/32"));
    assert!(options.routes.iter().any(|r| r == "11.11.11.12/32"));
    assert!(!options.routes.iter().any(|r| r.contains("255.")));
    assert_eq!(
        options.search_domains,
        vec![
            "sslvpn.sankuai.info".to_owned(),
            "sankuai.com".to_owned(),
            "meituan.net".to_owned(),
        ]
    );
}

#[test]
fn force_global_keeps_dns_host_routes() {
    let mut profile = ConnectionProfile::new_draft();
    profile.force_global = true;
    let network = NetworkSnapshot {
        address: Some("11.36.1.2".into()),
        netmask: Some("255.255.224.0".into()),
        address_v6: None,
        netmask_v6: None,
        gateway: None,
        dns: vec!["11.11.11.11".into()],
        mtu: 1300,
        routes: vec!["11.0.0.0/255.0.0.0".into()],
        split_excludes: vec![],
        domain: None,
        split_dns: vec![],
    };
    let options = VpnOptions::from_network(&network, &profile);
    assert_eq!(options.addresses, vec!["11.36.1.2/19".to_owned()]);
    assert!(options.routes.iter().any(|r| r == "0.0.0.0/0"));
    assert!(options.routes.iter().any(|r| r == "11.11.11.11/32"));
    // force_global is full tunnel only (ics), not server split list
    assert!(!options.routes.iter().any(|r| r == "11.0.0.0/8"));
}

#[test]
fn apply_force_global_sets_default_and_dns() {
    let mut options = VpnOptions {
        force_global: true,
        dns_addresses: vec!["11.11.11.11".into()],
        routes: vec!["11.0.0.0/8".into(), "10.20.205.51/32".into()],
        ..VpnOptions::default()
    };
    options.apply_force_global();
    assert!(options.routes.iter().any(|r| r == "0.0.0.0/0"));
    assert!(options.routes.iter().any(|r| r == "11.11.11.11/32"));
    assert_eq!(options.routes.len(), 2);
}

#[test]
fn slash32_netmask_clamped_for_platform() {
    let mut profile = ConnectionProfile::new_draft();
    profile.force_global = false;
    let network = NetworkSnapshot {
        address: Some("10.1.2.3".into()),
        netmask: Some("255.255.255.255".into()),
        address_v6: None,
        netmask_v6: None,
        gateway: None,
        dns: vec!["10.0.0.53".into()],
        mtu: 1400,
        routes: vec![],
        split_excludes: vec![],
        domain: None,
        split_dns: vec![],
    };
    let options = VpnOptions::from_network(&network, &profile);
    // OpenHarmony ParseAddress rejects prefix >= 32
    assert_eq!(options.addresses, vec!["10.1.2.3/31".to_owned()]);
    assert!(options.routes.iter().any(|r| r == "0.0.0.0/0"));
    assert!(options.routes.iter().any(|r| r == "10.0.0.53/32"));
}

#[test]
fn full_tunnel_handoff_preserves_local_lan_policy() {
    let mut profile = ConnectionProfile::new_draft();
    profile.force_global = true;
    profile.allow_local_lan = true;
    let options = VpnOptions::from_network(&NetworkSnapshot::default(), &profile);

    assert!(options.allow_local_lan);
    assert!(
        !options.allow_bypass,
        "full tunnel clears only the derived hint"
    );
    assert!(options
        .excluded_routes
        .iter()
        .any(|route| route == "10.0.0.0/8"));
}

#[test]
fn dns_host_routes_are_stable_and_deduplicated() {
    let mut options = VpnOptions {
        routes: vec!["0.0.0.0/0".into(), "10.10.10.1/32".into()],
        dns_addresses: vec!["10.10.10.1".into(), "2001:db8::53".into()],
        ..VpnOptions::default()
    };

    options.push_dns_host_routes();
    options.push_dns_host_routes();

    assert_eq!(
        options
            .routes
            .iter()
            .filter(|route| *route == "10.10.10.1/32")
            .count(),
        1
    );
    assert_eq!(
        options
            .routes
            .iter()
            .filter(|route| *route == "2001:db8::53/128")
            .count(),
        1
    );
}

#[test]
fn excluded_routes_materialize_with_longest_prefix_includes() {
    let mut options = VpnOptions {
        routes: vec![
            "10.10.10.1/32".into(),
            "0.0.0.0/0".into(),
            "2001:db8:1::53/128".into(),
            "::/0".into(),
        ],
        excluded_routes: vec!["10.0.0.0/8".into(), "2001:db8::/32".into()],
        ..VpnOptions::default()
    };

    options.materialize_excluded_routes().unwrap();

    assert!(options.excluded_routes.is_empty());
    assert!(route_policy_contains(&options.routes, "8.8.8.8"));
    assert!(!route_policy_contains(&options.routes, "10.20.30.40"));
    assert!(route_policy_contains(&options.routes, "10.10.10.1"));
    assert!(route_policy_contains(
        &options.routes,
        "2001:4860:4860::8888"
    ));
    assert!(!route_policy_contains(&options.routes, "2001:db8:2::1"));
    assert!(route_policy_contains(&options.routes, "2001:db8:1::53"));
}

#[test]
fn materializing_nested_excludes_is_order_independent() {
    let mut narrow_then_wide = VpnOptions {
        routes: vec!["0.0.0.0/0".into()],
        excluded_routes: vec!["10.1.0.0/16".into(), "10.0.0.0/8".into()],
        ..VpnOptions::default()
    };
    let mut wide_then_narrow = VpnOptions {
        routes: vec!["0.0.0.0/0".into()],
        excluded_routes: vec!["10.0.0.0/8".into(), "10.1.0.0/16".into()],
        ..VpnOptions::default()
    };

    narrow_then_wide.materialize_excluded_routes().unwrap();
    wide_then_narrow.materialize_excluded_routes().unwrap();

    assert_eq!(narrow_then_wide.routes, wide_then_narrow.routes);
    assert!(!route_policy_contains(&narrow_then_wide.routes, "10.2.0.1"));
}

#[test]
fn materializing_an_empty_policy_is_rejected_without_mutation() {
    let mut options = VpnOptions {
        routes: vec!["0.0.0.0/0".into()],
        excluded_routes: vec!["0.0.0.0/0".into()],
        ..VpnOptions::default()
    };
    let original = options.clone();

    let error = options.materialize_excluded_routes().unwrap_err();

    assert!(error.contains("remove every include"));
    assert_eq!(options, original);
}

#[test]
fn invalid_and_bare_ipv6_routes_are_handled_explicitly() {
    assert_eq!(
        normalize_route_cidr("2001:db8::1").as_deref(),
        Some("2001:db8::1/128")
    );
    let mut options = VpnOptions {
        routes: vec!["0.0.0.0/0".into()],
        excluded_routes: vec!["not-a-route".into()],
        ..VpnOptions::default()
    };
    assert!(options
        .materialize_excluded_routes()
        .unwrap_err()
        .contains("invalid VPN excluded"));
}

fn route_policy_contains(routes: &[String], address: &str) -> bool {
    let address = address.parse::<std::net::IpAddr>().unwrap();
    routes.iter().any(|route| {
        let prefix = IpPrefix::parse(route).unwrap();
        match address {
            std::net::IpAddr::V4(address) => prefix.contains(IpPrefix::V4 {
                network: u32::from(address),
                bits: 32,
            }),
            std::net::IpAddr::V6(address) => prefix.contains(IpPrefix::V6 {
                network: u128::from(address),
                bits: 128,
            }),
        }
    })
}
