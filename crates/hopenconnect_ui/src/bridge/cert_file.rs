//! `hopenconnect.cert-file` bridge plugin: client/CA certificate selection.
//!
//! ArkTS side owns the system `DocumentViewPicker` flow and copies the selected
//! file into the app sandbox (with the 8 MiB cap and `securePrivateFile`
//! hardening) before returning the real filesystem path OpenConnect can open.

use arkit::napi_derive_ohos::napi;
use arkit::openharmony_ability::{
    impl_bridge_napi_type, AsyncBridge, BridgeContextRequirement, BridgePlugin,
};

pub struct HOpenCertFileBridgePlugin;

impl BridgePlugin for HOpenCertFileBridgePlugin {
    type Mode = AsyncBridge;

    const ID: &'static str = "hopenconnect.cert-file";
    const REQUIRED_CONTEXTS: &'static [BridgeContextRequirement] =
        &[BridgeContextRequirement::Ability];
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct CertFileRequest {
    /// `"certificate"`, `"private_key"` or `"ca_certificate"`.
    pub kind: String,
}

impl_bridge_napi_type!(CertFileRequest, "hopenconnect.CertFileRequest");

#[napi(object)]
#[derive(Clone, Debug)]
pub struct CertFileResponse {
    /// Sandbox path of the copied certificate; empty when the user cancelled.
    pub path: String,
}

impl_bridge_napi_type!(CertFileResponse, "hopenconnect.CertFileResponse");

#[cfg(test)]
mod tests {
    use super::{CertFileRequest, CertFileResponse};
    use arkit::openharmony_ability::BridgeNapiType;

    #[test]
    fn cert_file_uses_stable_named_napi_contracts() {
        assert_eq!(
            <CertFileRequest as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.CertFileRequest"
        );
        assert_eq!(
            <CertFileResponse as BridgeNapiType>::TYPE_NAME,
            "hopenconnect.CertFileResponse"
        );
    }
}
