//! Owner-side release preparation. All output is public; keys are path-only inputs.
use super::{archive, bundle, *};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::atomic::AtomicBool,
};

/// Read a configured, nonempty verification key map.
pub fn keys(path: &Path) -> Result<KeyRing, String> {
    let keys = parse_keys(&bundle::read_small(path, 64 * 1024)?)?;
    if keys.is_empty() {
        return Err("configure publisher verification keys before packaging a release".into());
    }
    Ok(keys)
}
/// Export only the public verification map from an explicitly selected protected seed.
/// Never generate a key or print private bytes.
pub fn public_key(path: &Path, id: &str) -> Result<String, String> {
    let key = private_key(path)?;
    let document = serde_json::to_string(&std::collections::BTreeMap::from([(
        id,
        hex(key.verifying_key().as_bytes()),
    )]))
    .map_err(|_| "could not encode public key")?;
    parse_keys(document.as_bytes())?;
    Ok(document)
}
/// Describe exact CI bytes; this does not authorize or perform publication.
pub fn manifest(app: &Path, zip: &Path, repository: &str, output: &Path) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let build = bundle::info(app)?;
    bundle::validate(app, Some(&build), &cancel)?;
    keys(&app.join("Contents/Resources/update-keys.json"))?;
    let (bytes, sha256) = bundle::hash_file(zip, &cancel)?;
    let changelog = bundle::read_small(
        &app.join("Contents/Resources/CHANGELOG.md"),
        4 * 1024 * 1024,
    )?;
    let changelog = String::from_utf8(changelog).map_err(|_| "invalid release notes")?;
    let releases = crate::changelog::parse(&changelog);
    let release = releases
        .iter()
        .find(|r| Some(r.version.as_str()) == build.tag.as_deref())
        .ok_or("selected release has no changelog section")?;
    let notes = release.markdown_notes();
    let manifest = Manifest {
        schema: 1,
        app_id: APP_ID.into(),
        repository: repository.into(),
        archive: zip
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid archive filename")?
            .into(),
        bytes,
        sha256,
        build,
        notes,
    };
    manifest.validate(
        repository,
        manifest.build.tag.as_deref().ok_or("missing release tag")?,
        manifest.build.channel.ok_or("missing release channel")?,
    )?;
    archive::inspect(zip, &manifest, &cancel)?;
    bundle::write_new(
        output,
        &serde_json::to_vec_pretty(&manifest).map_err(|_| "could not encode manifest")?,
        false,
    )
}
fn unsigned(bytes: &[u8]) -> Result<Manifest, String> {
    let m: Manifest = serde_json::from_slice(bytes).map_err(|_| "invalid release manifest")?;
    m.validate(
        &m.repository,
        m.build.tag.as_deref().ok_or("missing release tag")?,
        m.build.channel.ok_or("missing release channel")?,
    )?;
    Ok(m)
}
fn private_key(path: &Path) -> Result<SigningKey, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "could not read publisher private key")?;
    let meta = file
        .metadata()
        .map_err(|_| "could not inspect publisher private key")?;
    // SAFETY: geteuid takes no arguments and returns the current owner's identity.
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() != 32
    {
        return Err("private key must be an owner-only regular 32-byte seed file".into());
    }
    let mut raw = [0u8; 32];
    file.read_exact(&mut raw)
        .map_err(|_| "could not read publisher private key")?;
    let key = SigningKey::from_bytes(&raw);
    raw.fill(0);
    Ok(key)
}
fn payload(
    zip: &Path,
    manifest: &Manifest,
) -> Result<(tempfile::TempDir, std::path::PathBuf), String> {
    let cancel = AtomicBool::new(false);
    let scratch = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|_| "could not prepare release inspection")?;
    let copy = scratch.path().join(&manifest.archive);
    bundle::copy_archive(zip, &copy, manifest.bytes, &cancel)?;
    let actual = bundle::hash_file(&copy, &cancel)?;
    if actual.0 != manifest.bytes || !actual.1.eq_ignore_ascii_case(&manifest.sha256) {
        return Err("archive bytes disagree with manifest".into());
    }
    let app = archive::extract(&copy, &scratch.path().join("extracted"), manifest, &cancel)
        .map_err(|e| e.to_string())?;
    Ok((scratch, app))
}
/// Inspect a copied payload and require the signing key in that exact app before signing.
/// Refuse existing output directories, insecure private-key files and mismatched bytes.
pub fn sign(
    manifest_path: &Path,
    zip: &Path,
    key_path: &Path,
    id: &str,
    output: &Path,
) -> Result<(), String> {
    if output.exists() {
        return Err("signature output already exists".into());
    }
    let bytes = bundle::read_small(manifest_path, MAX_MANIFEST)?;
    let manifest = unsigned(&bytes)?;
    let key = private_key(key_path)?;
    let (scratch, app) = payload(zip, &manifest)?;
    let keys = keys(&app.join("Contents/Resources/update-keys.json"))?;
    if keys.get(id) != Some(key.verifying_key().as_bytes()) {
        return Err("signing key is not embedded in the exact CI-built app".into());
    }
    let archive = scratch.path().join(&manifest.archive);
    let metadata_signature = sign_manifest(&bytes, &key, id)?;
    let archive_signature = sign_archive(bundle::open_regular(&archive)?, &key, id)?;
    // Reserve a fresh directory atomically. Existing paths are never replaced.
    fs::create_dir(output).map_err(|_| "signature output exists or cannot be created")?;
    bundle::copy(
        &archive,
        &output.join(&manifest.archive),
        &AtomicBool::new(false),
    )?;
    bundle::write_new(
        &output.join("guild-update.json.sig"),
        &metadata_signature,
        false,
    )?;
    bundle::write_new(
        &output.join(format!("{}.sig", manifest.archive)),
        &archive_signature,
        false,
    )?;
    // Metadata last: incomplete output cannot pass verify or become a valid release set.
    bundle::write_new(&output.join("guild-update.json"), &bytes, false)
}
/// Authenticate and validate a complete signed release set with separately trusted keys.
pub fn verify(directory: &Path, key_path: &Path) -> Result<(), String> {
    let keys = keys(key_path)?;
    let bytes = bundle::read_small(&directory.join("guild-update.json"), MAX_MANIFEST)?;
    let signature = bundle::read_small(&directory.join("guild-update.json.sig"), 4096)?;
    let manifest = verify_manifest(&bytes, &signature, &keys)?;
    let zip = directory.join(&manifest.archive);
    let signature = bundle::read_small(&directory.join(format!("{}.sig", manifest.archive)), 4096)?;
    let actual = verify_archive(bundle::open_regular(&zip)?, &signature, &keys)?;
    if actual.0 != manifest.bytes || !actual.1.eq_ignore_ascii_case(&manifest.sha256) {
        return Err("release archive digest or length mismatch".into());
    }
    let (_scratch, _app) = payload(&zip, &manifest)?;
    Ok(())
}
