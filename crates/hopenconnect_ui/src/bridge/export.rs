//! `hopenconnect.export` bridge plugin: export text (log archive) to a
//! user-chosen file through the system document picker with a pre-filled
//! suggested file name.

use arkit::napi_derive_ohos::napi;
use arkit::openharmony_ability::{
    impl_bridge_napi_type, AsyncBridge, BridgeContextRequirement, BridgePlugin,
};

pub struct HOpenExportBridgePlugin;

impl BridgePlugin for HOpenExportBridgePlugin {
    type Mode = AsyncBridge;

    const ID: &'static str = "hopenconnect.export";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct ExportTextRequest {
    pub suggested_name: String,
    pub content: String,
}

impl_bridge_napi_type!(ExportTextRequest, "hopenconnect.ExportTextRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct ExportTextResponse {}

impl_bridge_napi_type!(ExportTextResponse, "hopenconnect.ExportTextResponse");

#[cfg(test)]
mod tests {
    use super::{ExportTextRequest, ExportTextResponse};
    use arkit::openharmony_ability::BridgeNapiType;

    #[test]
    fn export_uses_stable_named_napi_contracts() {
        assert_eq!(
            <ExportTextRequest as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.ExportTextRequest"
        );
        assert_eq!(
            <ExportTextResponse as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.ExportTextResponse"
        );
    }
}
