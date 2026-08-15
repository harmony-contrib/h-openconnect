//! `hopenconnect.vpn` bridge plugin: system VPN extension control.
//!
//! ArkTS side owns the VPN start orchestration: `beginPlatformVpnStart`,
//! `startVpnExtensionAbility`, the first-authorization redispatch, and the
//! final `awaitPlatformVpnStart` outcome. Rust only submits the options JSON
//! and waits for the outcome.

use arkit::napi_derive_ohos::napi;
use arkit::openharmony_ability::{
    impl_bridge_napi_type, AsyncBridge, BridgeContextRequirement, BridgePlugin,
};

pub struct HOpenVpnBridgePlugin;

impl BridgePlugin for HOpenVpnBridgePlugin {
    type Mode = AsyncBridge;

    const ID: &'static str = "hopenconnect.vpn";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct VpnStartRequest {
    /// Serialized `VpnOptions` JSON for the extension Want.
    pub options_json: String,
}

impl_bridge_napi_type!(VpnStartRequest, "hopenconnect.VpnStartRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct VpnStartResponse {}

impl_bridge_napi_type!(VpnStartResponse, "hopenconnect.VpnStartResponse");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct VpnStopRequest {}

impl_bridge_napi_type!(VpnStopRequest, "hopenconnect.VpnStopRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct VpnStopResponse {}

impl_bridge_napi_type!(VpnStopResponse, "hopenconnect.VpnStopResponse");

#[cfg(test)]
mod tests {
    use super::{VpnStartRequest, VpnStartResponse, VpnStopRequest, VpnStopResponse};
    use arkit::openharmony_ability::BridgeNapiType;

    #[test]
    fn vpn_uses_stable_named_napi_contracts() {
        assert_eq!(
            <VpnStartRequest as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.VpnStartRequest"
        );
        assert_eq!(
            <VpnStartResponse as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.VpnStartResponse"
        );
        assert_eq!(
            <VpnStopRequest as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.VpnStopRequest"
        );
        assert_eq!(
            <VpnStopResponse as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.VpnStopResponse"
        );
    }
}
