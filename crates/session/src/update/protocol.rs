//! Versioned build identity and publisher signatures. No transport or secret storage.
use super::Channel;
pub use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{collections::BTreeMap, io::Read};
use uuid::Uuid;
/// Guild's immutable app identity.
pub const APP_ID: &str = "dev.guild.desktop";
/// Largest accepted downloaded archive.
pub const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;
/// Largest signed metadata document.
pub const MAX_MANIFEST: usize = 128 * 1024;
/// Trusted publisher keys embedded before selecting a production revision.
pub type KeyRing = BTreeMap<String, [u8; 32]>;
/// Origin of a build, independently of its selected update source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// A local development build.
    Local,
    /// An exact revision built by the manual GitHub workflow.
    GithubActions,
}
/// CI provenance attested by the owner's signature, not a notarization claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiIdentity {
    /// Repository owner/name.
    pub repository: String,
    /// GitHub workflow run.
    pub run_id: u64,
    /// Run attempt.
    pub attempt: u32,
}
/// Build resources written before bundle signing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildInfo {
    /// Format version.
    pub schema: u32,
    /// Unique artifact identity; rebuilds have different identities.
    pub build_id: Uuid,
    /// Full source commit.
    pub commit: String,
    /// Uncommitted source changes.
    pub dirty: bool,
    /// Exact selected tag, never arbitrary tag enumeration order.
    pub tag: Option<String>,
    /// Release channel, absent for dev builds.
    pub channel: Option<Channel>,
    /// Main first-parent source order, not build time.
    pub revision_order: Option<u64>,
    /// Build timestamp for display only.
    pub built: u64,
    /// Producer.
    pub provenance: Provenance,
    /// Target architecture.
    pub arch: String,
    /// Minimum macOS version.
    pub min_macos: String,
    /// CI source identity when applicable.
    pub ci: Option<CiIdentity>,
}
impl BuildInfo {
    /// New local artifact; tooling fills source metadata before signing.
    pub fn local(commit: String) -> Self {
        Self {
            schema: 1,
            build_id: Uuid::new_v4(),
            commit,
            dirty: false,
            tag: None,
            channel: None,
            revision_order: None,
            built: 0,
            provenance: Provenance::Local,
            arch: "arm64".into(),
            min_macos: "13.0".into(),
            ci: None,
        }
    }
    /// Validate identity without consulting a checkout.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.build_id.is_nil()
            || self.commit.len() != 40
            || !self.commit.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("invalid build identity or unsupported schema".into());
        }
        if self.arch != "arm64"
            || self
                .min_macos
                .split('.')
                .any(|p| p.is_empty() || p.parse::<u32>().is_err())
        {
            return Err("unsupported architecture or minimum system version".into());
        }
        if let Some(channel) = self.channel
            && (!self
                .tag
                .as_ref()
                .is_some_and(|t| channel.accepts(t, channel == Channel::Nightly, false))
                || self.revision_order.is_none_or(|n| n == 0))
        {
            return Err("invalid release tag/channel/order".into());
        }
        if self.provenance == Provenance::GithubActions
            && (self.dirty
                || self.channel.is_none()
                || self.ci.as_ref().is_none_or(|c| {
                    c.run_id == 0 || c.attempt == 0 || !repository_valid(&c.repository)
                }))
        {
            return Err("incomplete or dirty CI provenance".into());
        }
        Ok(())
    }
}
/// Relationship to the installed product, separate from artifact build times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// Exact same build artifact.
    Installed,
    /// A newer main revision.
    Newer,
    /// Reinstall, rollback or unknown/incomparable development revision.
    Replacement,
}
/// Compare authenticated source identity; caller supplies Local/Remote policy.
pub fn compare(
    running: Option<&BuildInfo>,
    candidate: &BuildInfo,
    local: bool,
) -> Result<Comparison, String> {
    let Some(r) = running else {
        return Ok(Comparison::Replacement);
    };
    if r.build_id == candidate.build_id {
        return if r == candidate {
            Ok(Comparison::Installed)
        } else {
            Err("build identity reused with different metadata".into())
        };
    }
    if local || r.commit == candidate.commit {
        return Ok(Comparison::Replacement);
    }
    match (r.revision_order, candidate.revision_order) {
        (Some(a), Some(b)) if a == b => {
            Err("different commits claim the same revision order".into())
        }
        (Some(a), Some(b)) if b > a => Ok(Comparison::Newer),
        _ => Ok(Comparison::Replacement),
    }
}
/// Signed release metadata; all fields are validated after signature verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format version.
    pub schema: u32,
    /// App bundle identity.
    pub app_id: String,
    /// Provider repository identity.
    pub repository: String,
    /// Embedded build identity.
    pub build: BuildInfo,
    /// Release asset basename, never an arbitrary URL.
    pub archive: String,
    /// Actual archive length.
    pub bytes: u64,
    /// SHA256 of archive bytes.
    pub sha256: String,
    /// Plain release notes.
    pub notes: String,
}
impl Manifest {
    /// Validate release metadata against the independently selected provider/release.
    pub fn validate(&self, repository: &str, tag: &str, channel: Channel) -> Result<(), String> {
        self.build.validate()?;
        if self.schema != 1
            || self.app_id != APP_ID
            || self.repository != repository
            || !repository_valid(repository)
            || self.build.tag.as_deref() != Some(tag)
            || self.build.channel != Some(channel)
            || self.build.provenance != Provenance::GithubActions
            || self.build.ci.as_ref().map(|c| c.repository.as_str()) != Some(repository)
        {
            return Err(
                "release metadata does not match the selected repository/tag/channel".into(),
            );
        }
        if self.bytes == 0
            || self.bytes > MAX_ARCHIVE
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || !self.archive.ends_with(".zip")
            || self.archive.contains(['/', '\\', ':'])
            || self.archive.contains("..")
            || self.notes.len() > 64 * 1024
        {
            return Err("invalid or oversized release asset metadata".into());
        }
        Ok(())
    }
}
/// Provider identities are exactly owner/name, without paths, URLs or shell syntax.
pub fn repository_valid(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
}
const MANIFEST_DOMAIN: &[u8] = b"Guild update manifest v1\0";
const ARCHIVE_CONTEXT: &[u8] = b"Guild update archive v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    algorithm: String,
    key_id: String,
    signature: String,
}
fn key_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
/// Lowercase hex for public identities, checksums and detached signatures.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Decode a bounded hex identity; callers must enforce the expected byte length.
pub fn unhex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() > 4096 || !value.len().is_multiple_of(2) {
        return Err("invalid hex identity".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            let nibble = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            Ok(nibble(p[0]).ok_or("invalid hex identity")? * 16
                + nibble(p[1]).ok_or("invalid hex identity")?)
        })
        .collect()
}
fn envelope(signature: Signature, id: &str, algorithm: &str) -> Result<Vec<u8>, String> {
    if !key_id_valid(id) {
        return Err("invalid publisher key id".into());
    }
    serde_json::to_vec(&Envelope {
        algorithm: algorithm.into(),
        key_id: id.into(),
        signature: hex(&signature.to_bytes()),
    })
    .map_err(|_| "could not encode detached signature".into())
}
fn public(
    signature: &[u8],
    keys: &KeyRing,
    algorithm: &str,
) -> Result<(VerifyingKey, Signature), String> {
    if signature.len() > 4096 {
        return Err("oversized detached signature".into());
    }
    let env: Envelope =
        serde_json::from_slice(signature).map_err(|_| "invalid detached signature envelope")?;
    if env.algorithm != algorithm || !key_id_valid(&env.key_id) {
        return Err("unsupported detached signature algorithm".into());
    }
    let bytes = keys.get(&env.key_id).ok_or("untrusted publisher key")?;
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| "invalid publisher public key")?;
    let raw = unhex(&env.signature)?;
    let sig = Signature::from_slice(&raw).map_err(|_| "invalid detached signature length")?;
    Ok((key, sig))
}
fn parsed(bytes: &[u8]) -> Result<Manifest, String> {
    if bytes.len() > MAX_MANIFEST {
        return Err("manifest exceeds size limit".into());
    }
    let m: Manifest = serde_json::from_slice(bytes).map_err(|_| "invalid release manifest")?;
    let tag = m.build.tag.as_deref().ok_or("release tag missing")?;
    let channel = m.build.channel.ok_or("release channel missing")?;
    m.validate(&m.repository, tag, channel)?;
    Ok(m)
}
/// Sign the exact UTF8 manifest document, not a reserialized approximation.
pub fn sign_manifest(bytes: &[u8], key: &SigningKey, id: &str) -> Result<Vec<u8>, String> {
    parsed(bytes)?;
    let mut message = Vec::with_capacity(MANIFEST_DOMAIN.len() + bytes.len());
    message.extend_from_slice(MANIFEST_DOMAIN);
    message.extend_from_slice(bytes);
    envelope(key.sign(&message), id, "ed25519")
}
/// Authenticate bytes before interpreting metadata.
pub fn verify_manifest(bytes: &[u8], signature: &[u8], keys: &KeyRing) -> Result<Manifest, String> {
    if bytes.len() > MAX_MANIFEST {
        return Err("manifest exceeds size limit".into());
    }
    let (key, sig) = public(signature, keys, "ed25519")?;
    let mut message = Vec::with_capacity(MANIFEST_DOMAIN.len() + bytes.len());
    message.extend_from_slice(MANIFEST_DOMAIN);
    message.extend_from_slice(bytes);
    key.verify_strict(&message, &sig)
        .map_err(|_| "publisher manifest signature does not match")?;
    parsed(bytes)
}
fn digests(mut input: impl Read) -> Result<(Sha512, u64, String), String> {
    let mut full = Sha512::new();
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input
            .read(&mut buffer)
            .map_err(|_| "archive could not be read")?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or("archive exceeds size limit")?;
        if total > MAX_ARCHIVE {
            return Err("archive exceeds size limit".into());
        }
        full.update(&buffer[..n]);
        hash.update(&buffer[..n]);
    }
    if total == 0 {
        return Err("empty update archive".into());
    }
    Ok((full, total, hex(&hash.finalize())))
}
/// Stream-sign an archive with RFC8032 Ed25519ph/SHA512 and a fixed context.
pub fn sign_archive(input: impl Read, key: &SigningKey, id: &str) -> Result<Vec<u8>, String> {
    let (digest, _, _) = digests(input)?;
    let signature = key
        .sign_prehashed(digest, Some(ARCHIVE_CONTEXT))
        .map_err(|_| "could not sign update archive")?;
    envelope(signature, id, "ed25519ph-sha512")
}
/// Authenticate archive bytes before extraction, returning actual length and SHA256.
pub fn verify_archive(
    input: impl Read,
    signature: &[u8],
    keys: &KeyRing,
) -> Result<(u64, String), String> {
    let (key, sig) = public(signature, keys, "ed25519ph-sha512")?;
    let (digest, len, hash) = digests(input)?;
    key.verify_prehashed_strict(digest, Some(ARCHIVE_CONTEXT), &sig)
        .map_err(|_| "publisher archive signature does not match")?;
    Ok((len, hash))
}
/// Parse embedded public keys without accepting private key material or unknown fields.
pub fn parse_keys(bytes: &[u8]) -> Result<KeyRing, String> {
    if bytes.len() > 64 * 1024 {
        return Err("publisher keys exceed size limit".into());
    }
    let raw: BTreeMap<String, String> =
        serde_json::from_slice(bytes).map_err(|_| "invalid publisher public keys")?;
    let mut keys = KeyRing::new();
    for (id, value) in raw {
        if !key_id_valid(&id) {
            return Err("invalid publisher key id".into());
        }
        let raw = unhex(&value)?;
        let bytes: [u8; 32] = raw
            .try_into()
            .map_err(|_| "invalid publisher public key length")?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid publisher public key")?;
        if key.is_weak() {
            return Err("weak publisher public key refused".into());
        }
        keys.insert(id, bytes);
    }
    Ok(keys)
}
#[cfg(test)]
mod tests {
    use super::*;
    const COMMIT: &str = "1111111111111111111111111111111111111111";
    fn manifest() -> Manifest {
        let mut build = BuildInfo::local(COMMIT.into());
        build.tag = Some("nightly-2026-10-18".into());
        build.channel = Some(Channel::Nightly);
        build.revision_order = Some(50);
        build.provenance = Provenance::GithubActions;
        build.ci = Some(CiIdentity {
            repository: "owner/guild".into(),
            run_id: 1,
            attempt: 1,
        });
        Manifest {
            schema: 1,
            app_id: APP_ID.into(),
            repository: "owner/guild".into(),
            build,
            archive: "Guild-nightly.zip".into(),
            bytes: 6,
            sha256: "00".repeat(32),
            notes: "Fixed a visible problem".into(),
        }
    }
    #[test]
    fn signed_metadata_and_archive_authenticate_exact_bytes_and_reject_substitution() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut keys = KeyRing::new();
        keys.insert("fixture".into(), key.verifying_key().to_bytes());
        let m = manifest();
        let bytes = serde_json::to_vec(&m).unwrap();
        let sig = sign_manifest(&bytes, &key, "fixture").unwrap();
        assert_eq!(verify_manifest(&bytes, &sig, &keys).unwrap(), m);
        let mut changed = bytes.clone();
        changed.push(b' ');
        assert!(verify_manifest(&changed, &sig, &keys).is_err());
        let archive = b"bundle";
        let sig = sign_archive(&archive[..], &key, "fixture").unwrap();
        assert_eq!(verify_archive(&archive[..], &sig, &keys).unwrap().0, 6);
        assert!(verify_archive(&b"bundlf"[..], &sig, &keys).is_err());
        assert!(
            verify_manifest(&bytes, &sig, &keys).is_err(),
            "archive signature cannot authenticate metadata"
        );
    }
    #[test]
    fn release_channels_and_signed_identity_are_bound_to_the_selected_release() {
        let mut m = manifest();
        assert!(
            m.validate("owner/guild", "nightly-2026-10-18", Channel::Nightly)
                .is_ok()
        );
        assert!(
            m.validate("other/guild", "nightly-2026-10-18", Channel::Nightly)
                .is_err()
        );
        m.build.tag = Some("v1.0.0".into());
        m.build.channel = Some(Channel::Stable);
        assert!(m.validate("owner/guild", "v1.0.0", Channel::Stable).is_ok());
        assert!(!Channel::Stable.accepts("v1.0.0", true, false));
        assert!(!Channel::Nightly.accepts("v1.0.0", false, false));
    }
    #[test]
    fn rebuilding_or_promoting_the_same_commit_never_makes_a_newer_product() {
        let mut a = manifest().build;
        let mut b = a.clone();
        b.build_id = Uuid::new_v4();
        b.built = a.built + 10000;
        assert_eq!(
            compare(Some(&a), &b, false).unwrap(),
            Comparison::Replacement
        );
        b.commit = "2222222222222222222222222222222222222222".into();
        b.revision_order = Some(51);
        assert_eq!(compare(Some(&a), &b, false).unwrap(), Comparison::Newer);
        assert_eq!(
            compare(Some(&a), &b, true).unwrap(),
            Comparison::Replacement
        );
        a.revision_order = None;
        assert_eq!(
            compare(Some(&a), &b, false).unwrap(),
            Comparison::Replacement
        );
    }
    #[test]
    fn valid_signatures_do_not_override_schema_key_or_architecture_policy() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let keys = KeyRing::from([("fixture".into(), key.verifying_key().to_bytes())]);
        let m = manifest();
        let bytes = serde_json::to_vec(&m).unwrap();
        let sig = sign_manifest(&bytes, &key, "fixture").unwrap();
        assert!(verify_manifest(&bytes, &sig, &KeyRing::new()).is_err());
        let wrong = SigningKey::from_bytes(&[8; 32]);
        assert!(
            verify_manifest(
                &bytes,
                &sig,
                &KeyRing::from([("fixture".into(), wrong.verifying_key().to_bytes())])
            )
            .is_err()
        );
        let signed_raw = |raw: &[u8]| {
            let mut message = MANIFEST_DOMAIN.to_vec();
            message.extend_from_slice(raw);
            envelope(key.sign(&message), "fixture", "ed25519").unwrap()
        };
        let mut future = m.clone();
        future.schema = 99;
        let mut wrong_arch = m.clone();
        wrong_arch.build.arch = "x86_64".into();
        let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"schema\":1",
            "\"schema\":1,\"schema\":1",
            1,
        );
        for raw in [
            serde_json::to_vec(&future).unwrap(),
            serde_json::to_vec(&wrong_arch).unwrap(),
            duplicate.into_bytes(),
            b"not json".to_vec(),
        ] {
            assert!(verify_manifest(&raw, &signed_raw(&raw), &keys).is_err());
        }
        assert!(verify_manifest(&vec![b' '; MAX_MANIFEST + 1], &sig, &keys).is_err());
    }
}
