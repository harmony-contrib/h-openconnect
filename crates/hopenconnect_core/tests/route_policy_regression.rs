use hopenconnect_core::VpnOptions;
use std::net::IpAddr;

// Deliberately independent of the implementation's prefix splitting: compare
// the policy's longest-prefix decision with membership in the emitted routes.
fn matching_prefix(route: &str, address: IpAddr) -> Option<u8> {
    let (network, prefix) = route.split_once('/')?;
    let network: IpAddr = network.parse().ok()?;
    let bits: u8 = prefix.parse().ok()?;
    match (network, address) {
        (IpAddr::V4(network), IpAddr::V4(address)) if bits <= 32 => {
            let shift = 32 - u32::from(bits);
            let network = u64::from(u32::from(network)) >> shift;
            let address = u64::from(u32::from(address)) >> shift;
            (network == address).then_some(bits)
        }
        (IpAddr::V6(network), IpAddr::V6(address)) if bits <= 128 => {
            if bits == 0 {
                return Some(0);
            }
            let shift = 128 - u32::from(bits);
            (u128::from(network) >> shift == u128::from(address) >> shift).then_some(bits)
        }
        _ => None,
    }
}

fn policy_includes(includes: &[String], excludes: &[String], address: IpAddr) -> bool {
    let include = includes
        .iter()
        .filter_map(|route| matching_prefix(route, address))
        .max();
    let exclude = excludes
        .iter()
        .filter_map(|route| matching_prefix(route, address))
        .max();
    match (include, exclude) {
        (Some(include), Some(exclude)) => include > exclude,
        (Some(_), None) => true,
        _ => false,
    }
}

fn assert_policy_equivalent(includes: Vec<String>, excludes: Vec<String>, addresses: &[IpAddr]) {
    let mut options = VpnOptions {
        routes: includes.clone(),
        excluded_routes: excludes.clone(),
        ..VpnOptions::default()
    };
    let result = options.materialize_excluded_routes();
    let any_included = addresses
        .iter()
        .any(|address| policy_includes(&includes, &excludes, *address));
    assert_eq!(
        result.is_ok(),
        any_included,
        "empty-policy result differs: includes={includes:?}, excludes={excludes:?}, result={result:?}"
    );
    if result.is_err() {
        return;
    }
    assert!(options.excluded_routes.is_empty());
    for address in addresses {
        let expected = policy_includes(&includes, &excludes, *address);
        let actual = options
            .routes
            .iter()
            .any(|route| matching_prefix(route, *address).is_some());
        assert_eq!(
            actual, expected,
            "address={address}, includes={includes:?}, excludes={excludes:?}, output={:?}",
            options.routes
        );
    }
}

fn next(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed
}

#[test]
fn materialized_ipv4_matches_longest_prefix_policy_for_every_address() {
    let addresses: Vec<IpAddr> = (0..256)
        .map(|last| IpAddr::from([192, 0, 2, last as u8]))
        .collect();
    let mut seed = 0x1357_2468;
    for _ in 0..128 {
        let mut includes = vec!["192.0.2.0/24".to_owned()];
        let mut excludes = Vec::new();
        for _ in 0..6 {
            let address = next(&mut seed) % 256;
            let prefix = 24 + next(&mut seed) % 9;
            let route = format!("192.0.2.{address}/{prefix}");
            if next(&mut seed) & 3 == 0 {
                includes.push(route);
            } else {
                excludes.push(route);
            }
        }
        assert_policy_equivalent(includes.clone(), excludes.clone(), &addresses);
        excludes.reverse();
        assert_policy_equivalent(includes, excludes, &addresses);
    }
}

#[test]
fn materialized_ipv6_matches_longest_prefix_policy_for_every_address() {
    let base = u128::from("2001:db8::".parse::<std::net::Ipv6Addr>().unwrap());
    let addresses: Vec<IpAddr> = (0..256)
        .map(|last| IpAddr::V6(std::net::Ipv6Addr::from(base + last)))
        .collect();
    let mut seed = 0x2468_1357;
    for _ in 0..128 {
        let mut includes = vec!["2001:db8::/120".to_owned()];
        let mut excludes = Vec::new();
        for _ in 0..6 {
            let address = next(&mut seed) % 256;
            let prefix = 120 + next(&mut seed) % 9;
            let route = format!("2001:db8::{address:x}/{prefix}");
            if next(&mut seed) & 3 == 0 {
                includes.push(route);
            } else {
                excludes.push(route);
            }
        }
        assert_policy_equivalent(includes.clone(), excludes.clone(), &addresses);
        excludes.reverse();
        assert_policy_equivalent(includes, excludes, &addresses);
    }
}

#[test]
fn overlapping_exclusions_cannot_reinclude_a_subnet() {
    let includes = vec!["10.0.0.0/8".to_owned(), "10.10.10.1/32".to_owned()];
    let excludes = vec!["10.1.0.0/16".to_owned(), "10.0.0.0/8".to_owned()];
    let addresses = [
        "10.0.0.1",
        "10.1.0.1",
        "10.2.0.1",
        "10.10.10.1",
        "10.255.0.1",
    ]
    .map(|address| address.parse().unwrap());
    assert_policy_equivalent(includes, excludes, &addresses);
}
