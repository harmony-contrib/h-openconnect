//! `ohos.url` bridge plugin: open an external URL through the system link
//! opener. The ArkTS side is the built-in `UrlPlugin` from
//! `@ohos-rs/ability-plugin-url`; this is the Rust call facade, vendored so the
//! app tracks the `openharmony-ability` release selected by Arkit without
//! waiting for a matching plugin-url crates.io release.

use arkit::napi_derive_ohos::napi;
use arkit::openharmony_ability::{
    impl_bridge_napi_type, AsyncBridge, BridgeContextRequirement, BridgePlugin,
};

pub struct HOpenUrlBridgePlugin;

impl BridgePlugin for HOpenUrlBridgePlugin {
    type Mode = AsyncBridge;

    const ID: &'static str = "ohos.url";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct UrlOpenRequest {
    pub url: String,
}

impl_bridge_napi_type!(UrlOpenRequest, "ohos.url.OpenRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct UrlOpenResponse {
    pub accepted: bool,
}

impl_bridge_napi_type!(UrlOpenResponse, "ohos.url.OpenResponse");

#[cfg(test)]
mod tests {
    use super::{UrlOpenRequest, UrlOpenResponse};
    use arkit::openharmony_ability::BridgeNapiType;

    #[test]
    fn url_uses_stable_named_napi_contracts() {
        assert_eq!(
            <UrlOpenRequest as BridgeNapiType>::TYPE_NAME,
            "ohos.url.OpenRequest"
        );
        assert_eq!(
            <UrlOpenResponse as BridgeNapiType>::TYPE_NAME,
            "ohos.url.OpenResponse"
        );
    }
}
