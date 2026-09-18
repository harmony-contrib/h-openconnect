//! `hopenconnect.scan` bridge plugin: receive a QR payload from ScanKit.

use arkit::napi_derive_ohos::napi;
use arkit::openharmony_ability::{
    impl_bridge_napi_type, AsyncBridge, BridgeContextRequirement, BridgePlugin,
};

pub struct HOpenScanBridgePlugin;

impl BridgePlugin for HOpenScanBridgePlugin {
    type Mode = AsyncBridge;

    const ID: &'static str = "hopenconnect.scan";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct ScanRequest {}

impl_bridge_napi_type!(ScanRequest, "hopenconnect.ScanRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct ScanResponse {
    /// Empty when the user cancels the system scanner.
    pub content: String,
}

impl_bridge_napi_type!(ScanResponse, "hopenconnect.ScanResponse");
