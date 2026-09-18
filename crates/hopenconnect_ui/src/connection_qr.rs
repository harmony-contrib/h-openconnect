use crate::model::VpnConnection;
use serde::{Deserialize, Serialize};

const FORMAT: &str = "h-openconnect.connection";
const VERSION: u8 = 1;
const MAX_SCANNED_BYTES: usize = 8 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionQrPayload {
    format: String,
    version: u8,
    profile: VpnConnection,
}

pub(crate) fn encode_connection_qr(profile: &VpnConnection) -> Result<String, String> {
    profile.validate()?;
    let mut transferable = profile.clone();
    transferable.id.clear();
    serde_json::to_string(&ConnectionQrPayload {
        format: FORMAT.to_owned(),
        version: VERSION,
        profile: transferable,
    })
    .map_err(|_| "could not encode the connection QR code".to_owned())
}

pub(crate) fn decode_connection_qr(scanned: &str) -> Result<VpnConnection, String> {
    let scanned = scanned.trim_start_matches('\u{feff}').trim();
    if scanned.len() > MAX_SCANNED_BYTES {
        return Err("connection QR code is too large".to_owned());
    }
    let mut payload: ConnectionQrPayload = serde_json::from_str(scanned)
        .map_err(|_| "QR code does not contain an H-OpenConnect connection".to_owned())?;
    if payload.format != FORMAT || payload.version != VERSION {
        return Err("unsupported H-OpenConnect connection QR version".to_owned());
    }
    payload.profile.id.clear();
    payload.profile.validate()?;
    Ok(payload.profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> VpnConnection {
        let mut profile = VpnConnection::new_draft();
        profile.id = "existing-profile".to_owned();
        profile.name = "Work VPN".to_owned();
        profile.server = "vpn.example.com".to_owned();
        profile.username = "alice".to_owned();
        profile.password = "secret-password".to_owned();
        profile.token_string = "token-secret".to_owned();
        profile.certificate = "/data/app/cert.pem".to_owned();
        profile.force_global = true;
        profile
    }

    #[test]
    fn round_trip_preserves_settings_but_never_imports_an_existing_id() {
        let original = example();
        let encoded = encode_connection_qr(&original).unwrap();
        let imported = decode_connection_qr(&encoded).unwrap();
        assert_eq!(imported.id, "");
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.server, original.server);
        assert_eq!(imported.password, original.password);
        assert_eq!(imported.token_string, original.token_string);
        assert_eq!(imported.certificate, original.certificate);
        assert!(imported.force_global);
    }

    #[test]
    fn rejects_unrelated_or_invalid_connection_data() {
        let encoded = encode_connection_qr(&example()).unwrap();
        assert!(decode_connection_qr("https://vpn.example.com").is_err());
        assert!(
            decode_connection_qr(&encoded.replace("h-openconnect.connection", "other.app"))
                .is_err()
        );
        assert!(decode_connection_qr(&encoded.replace("\"version\":1", "\"version\":2")).is_err());
        assert!(decode_connection_qr(&encoded.replace("vpn.example.com", "")).is_err());
    }

    #[test]
    fn scanned_id_cannot_replace_an_existing_connection() {
        let encoded = encode_connection_qr(&example()).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        json["profile"]["id"] = serde_json::Value::String("existing-profile".to_owned());
        let imported = decode_connection_qr(&json.to_string()).unwrap();
        assert!(imported.id.is_empty());
    }
}
