// SPDX-License-Identifier: AGPL-3.0-or-later
// VoidVeil — tls.rs
// Self-signed cert generation for localhost E2E encryption.
// Generated fresh on first run. Stored at ~/.voidveil/{cert,key}.pem.
// Install cert once as trusted CA — all local traffic is encrypted.

use rcgen::{generate_simple_self_signed, CertifiedKey};
use std::path::PathBuf;

pub struct VoidCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

pub fn ensure_cert() -> VoidCert {
    let dir = dirs();
    std::fs::create_dir_all(&dir).ok();

    let cert_path = dir.join("cert.pem");
    let key_path  = dir.join("key.pem");

    // Reuse if already generated
    if cert_path.exists() && key_path.exists() {
        let cert_pem = std::fs::read_to_string(&cert_path).unwrap();
        let key_pem  = std::fs::read_to_string(&key_path).unwrap();
        return VoidCert { cert_pem, key_pem, cert_path, key_path };
    }

    // Generate new self-signed cert for localhost
    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ];
    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(subject_alt_names).unwrap();

    let cert_pem = cert.pem();
    let key_pem  = key_pair.serialize_pem();

    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path,  &key_pem).unwrap();

    tracing::info!("Generated TLS cert: {}", cert_path.display());
    tracing::info!("Install cert as trusted CA to suppress browser warnings:");
    tracing::info!("  Android: Settings → Security → Install certificate");
    tracing::info!("  Linux:   sudo cp {} /usr/local/share/ca-certificates/ && sudo update-ca-certificates", cert_path.display());

    VoidCert { cert_pem, key_pem, cert_path, key_path }
}

fn dirs() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".voidveil")
}
