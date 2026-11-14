//! Atomic identity bundle. Migration preserves the existing certificate and private key.
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use zeroize::{Zeroize, Zeroizing};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    cert: Vec<u8>,
    key: Vec<u8>,
}
impl Drop for Bundle {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
pub fn signature(key: &[u8], bytes: &[u8]) -> Result<String, String> {
    let random = SystemRandom::new();
    let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, key, &random)
        .map_err(|_| "Invalid device signing key")?;
    key.sign(&random, bytes)
        .map(|s| STANDARD.encode(s.as_ref()))
        .map_err(|_| "Device signing failed".into())
}
pub fn verify(cert: &[u8], bytes: &[u8], signature: &str) -> Result<(), String> {
    if cert.len() > 16384 || signature.len() > 192 {
        return Err("Oversized certificate or signature".into());
    }
    let der = rustls::pki_types::CertificateDer::from(cert);
    let cert = webpki::EndEntityCert::try_from(&der).map_err(|_| "Invalid device certificate")?;
    let signature = STANDARD
        .decode(signature)
        .map_err(|_| "Invalid revision signature")?;
    cert.verify_signature(webpki::ring::ECDSA_P256_SHA256, bytes, &signature)
        .map_err(|_| "Revision signature verification failed".into())
}
pub fn load(root: &Path) -> Result<(Vec<u8>, Zeroizing<Vec<u8>>), String> {
    if !crate::vault::unlocked() {
        return Err("Protected data is locked; Send/Sync is unavailable".into());
    }
    let path = root.join("identity.v153.json");
    let replace = root.join("replace-identity.request");
    let rotate = if replace.exists() {
        let request = crate::limits::read(&replace, 128).map_err(|e| e.to_string())?;
        let previous =
            Zeroizing::new(crate::limits::read(&path, 32768).map_err(|e| e.to_string())?);
        let previous: Bundle = serde_json::from_slice(&previous)
            .map_err(|_| "Invalid existing identity; replacement refused")?;
        if request == format!("replace:{}", crate::peer::hash(&previous.cert)).as_bytes() {
            true
        } else {
            fs::remove_file(&replace).map_err(|e| e.to_string())?;
            false
        }
    } else {
        false
    };
    let bundle = if path.exists() && !rotate {
        let bytes = Zeroizing::new(crate::limits::read(&path, 32768).map_err(|e| e.to_string())?);
        serde_json::from_slice::<Bundle>(&bytes).map_err(|_| "Invalid protected identity bundle")?
    } else if root.join("identity.der").exists() && !rotate {
        Bundle {
            cert: crate::limits::read(&root.join("identity.der"), 16384)
                .map_err(|e| e.to_string())?,
            key: crate::limits::read(&root.join("identity.key"), 16384)
                .map_err(|e| e.to_string())?,
        }
    } else {
        if !rotate && root.join("identity.key").exists() {
            return Err(
                "Incomplete existing identity; restore it instead of generating a replacement"
                    .into(),
            );
        }
        let identity = rcgen::generate_simple_self_signed(vec!["editortool.local".into()])
            .map_err(|e| e.to_string())?;
        Bundle {
            cert: identity.cert.der().to_vec(),
            key: identity.key_pair.serialize_der(),
        }
    };
    let challenge = b"EditorTool identity consistency v1";
    verify(&bundle.cert, challenge, &signature(&bundle.key, challenge)?)?;
    if !path.exists() || rotate {
        let bytes = Zeroizing::new(serde_json::to_vec(&bundle).map_err(|e| e.to_string())?);
        crate::session::atomic_write(&path, &bytes).map_err(|e| e.to_string())?;
        let stored = Zeroizing::new(crate::limits::read(&path, 32768).map_err(|e| e.to_string())?);
        if *stored != *bytes {
            return Err("Identity migration verification failed; legacy identity preserved".into());
        }
        for old in ["identity.key", "identity.der", "replace-identity.request"] {
            let old = root.join(old);
            if old.exists() {
                fs::remove_file(old).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok((bundle.cert.clone(), Zeroizing::new(bundle.key.clone())))
}
