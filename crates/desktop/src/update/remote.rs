//! Remote release discovery. Provider identity never comes from the open checkout.
use gpui::http_client::Url;
mod cache;
mod download;
pub use cache::CacheDraft;
mod gh;
mod idle;
mod verified;
use guild_session::update::{Channel, KeyRing, Manifest};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Sanitized transport/discovery failures; never contain tokens, raw headers or tool output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Request failed or returned an unsupported response.
    Network,
    /// Work was cancelled.
    Cancelled,
    /// Repository may be private or missing; authorized gh access is required.
    Access,
    /// A bounded enumeration/response limit was reached.
    Limit,
    /// Malformed or inconsistent provider identity.
    Invalid,
    /// Requests are suspended until this UTC epoch second.
    RateLimited(u64),
    /// gh is not installed on the login PATH.
    MissingGh,
    /// An owned download/cache file could not be written.
    Storage,
    /// Payload signature, digest or native bundle validation failed.
    Verification,
    /// Enough space could not be reserved on the update filesystem.
    Space,
}
/// Authenticated manifest pinned to exact release and asset identities.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// GitHub release identity.
    pub release_id: u64,
    /// Signed metadata.
    pub manifest: Manifest,
    /// Exact archive asset identity, not a metadata-supplied URL.
    pub archive_id: u64,
    /// Exact detached archive signature asset identity.
    pub signature_id: u64,
    manifest_bytes: Vec<u8>,
    manifest_signature: Vec<u8>,
}
impl Candidate {
    /// Exact artifact authority shared by cache and installation confirmation.
    pub fn identity(&self) -> Option<crate::update::control::Identity> {
        Some(crate::update::control::Identity {
            artifact: crate::update::control::Artifact::Build(self.manifest.build.build_id),
            origin: crate::update::control::Origin::Remote {
                repository: self.manifest.repository.clone(),
                channel: self.manifest.build.channel?,
                release_id: self.release_id,
                sha256: self.manifest.sha256.clone(),
            },
        })
    }
}
/// A complete chosen-channel discovery result.
#[derive(Debug)]
pub struct Discovery {
    /// Verified candidates in descending product order.
    pub candidates: Vec<Candidate>,
    /// Matching releases that could not be verified; never installable.
    pub unverified: usize,
}
/// Public progress contains byte counts and phase only, never URLs or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Streaming payload bytes under the signed length limit.
    Downloading {
        /// Completed byte count.
        bytes: u64,
        /// Signed total length.
        total: u64,
    },
    /// Publisher signature, digest, archive and native bundle verification.
    Verifying,
}
/// Validated owned download; dropping a cancelled/stale result removes its files.
#[derive(Debug)]
pub struct Acquired {
    directory: tempfile::TempDir,
    bundle: std::path::PathBuf,
    candidate: Candidate,
    seal: String,
}
impl Acquired {
    /// Fully validated payload app, never a path supplied by release metadata.
    pub fn bundle(&self) -> &std::path::Path {
        &self.bundle
    }
    /// Verified code seal captured as part of authenticated extraction.
    pub fn seal(&self) -> &str {
        &self.seal
    }
    /// Provenance and authenticated release identity for final preparation.
    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }
    /// Owned work directory retained until the consumer accepts or drops this result.
    pub fn work_dir(&self) -> &std::path::Path {
        self.directory.path()
    }
}
#[derive(Clone, Copy)]
enum ListingTransport {
    Anonymous,
    Authorized,
}

/// First Remote provider; public requests do not require gh or a source tree.
pub struct Provider {
    repository: String,
    keys: KeyRing,
    origin: String,
    verified: std::sync::Mutex<verified::Metadata>,
    #[cfg(test)]
    gh_override: Option<std::path::PathBuf>,
}
impl Provider {
    /// Bind a build's explicit provider identity and independently trusted publisher keys.
    pub fn new(repository: &str, keys: KeyRing) -> Result<Self, Error> {
        if !guild_session::update::repository_valid(repository) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            repository: repository.into(),
            keys,
            origin: "https://api.github.com".into(),
            verified: Default::default(),
            #[cfg(test)]
            gh_override: None,
        })
    }
    /// Discover only the chosen channel, authenticating metadata before product ordering.
    pub fn discover(&self, channel: Channel, cancel: &AtomicBool) -> Result<Discovery, Error> {
        check_cancel(cancel)?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let matching = self.list_releases(channel, cancel, deadline)?;
        let mut result = Discovery {
            candidates: Vec::new(),
            unverified: 0,
        };
        // GitHub order is unsigned and cannot bound signed revision_order. Verify
        // every matching release, overlapping at most four metadata pairs. Scoped
        // workers share the operation deadline/cancellation and are always joined.
        for batch in matching.chunks(4) {
            check_cancel(cancel)?;
            let verified = std::thread::scope(|scope| {
                let workers = batch
                    .iter()
                    .map(|release| {
                        std::thread::Builder::new()
                            .name("update-metadata".into())
                            .spawn_scoped(scope, move || {
                                check_cancel(cancel)?;
                                if Instant::now() >= deadline {
                                    return Err(Error::Network);
                                }
                                self.verify_release(release, channel, cancel, deadline)
                            })
                            .map_err(|_| Error::Network)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, Error>(
                    workers
                        .into_iter()
                        .map(|worker| worker.join().unwrap_or(Err(Error::Network)))
                        .collect::<Vec<_>>(),
                )
            })?;
            check_cancel(cancel)?;
            for candidate in verified {
                match candidate {
                    Ok(candidate) => result.candidates.push(candidate),
                    Err(Error::Invalid) => result.unverified += 1,
                    Err(error) => return Err(error),
                }
            }
        }
        let mut orders = std::collections::BTreeMap::new();
        for candidate in &result.candidates {
            let order = candidate
                .manifest
                .build
                .revision_order
                .ok_or(Error::Invalid)?;
            if orders
                .insert(order, &candidate.manifest.build.commit)
                .is_some_and(|commit| commit != &candidate.manifest.build.commit)
            {
                return Err(Error::Invalid);
            }
        }
        result.candidates.sort_by(|a, b| {
            b.manifest
                .build
                .revision_order
                .cmp(&a.manifest.build.revision_order)
                .then(b.release_id.cmp(&a.release_id))
        });
        Ok(result)
    }
    fn list_releases(
        &self,
        channel: Channel,
        cancel: &AtomicBool,
        deadline: Instant,
    ) -> Result<Vec<Release>, Error> {
        match self.list_with_transport(channel, cancel, deadline, ListingTransport::Anonymous) {
            Err(anonymous @ (Error::Access | Error::RateLimited(_))) => {
                // Authenticated listings can include drafts, shifting every page.
                // Discard the partial anonymous pass and restart at page 1 with
                // one visibility for the whole pass and the original deadline.
                match self.list_with_transport(
                    channel,
                    cancel,
                    deadline,
                    ListingTransport::Authorized,
                ) {
                    Err(error @ (Error::Access | Error::MissingGh)) => {
                        if matches!(anonymous, Error::RateLimited(_)) {
                            return Err(anonymous);
                        }
                        // Preserve public recovery from stale gh credentials. This
                        // final full anonymous pass cannot switch back to gh.
                        match self.list_with_transport(
                            channel,
                            cancel,
                            deadline,
                            ListingTransport::Anonymous,
                        ) {
                            Err(Error::Access) => Err(error),
                            other => other,
                        }
                    }
                    other => other,
                }
            }
            other => other,
        }
    }
    fn list_with_transport(
        &self,
        channel: Channel,
        cancel: &AtomicBool,
        deadline: Instant,
        transport: ListingTransport,
    ) -> Result<Vec<Release>, Error> {
        let mut matching = Vec::new();
        let mut ids = BTreeSet::new();
        for page in 1..=10 {
            check_cancel(cancel)?;
            if Instant::now() >= deadline {
                return Err(Error::Network);
            }
            let path = format!(
                "/repos/{}/releases?per_page=100&page={page}",
                self.repository
            );
            let request = Request {
                path: &path,
                limit: 4 * 1024 * 1024,
                asset: false,
                timeout: Duration::from_secs(15).min(
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or(Error::Network)?,
                ),
            };
            let network = crate::network::snapshot();
            let mut bytes = Vec::new();
            match transport {
                ListingTransport::Anonymous => {
                    self.http_stream(&request, cancel, &mut bytes, &mut |_| {}, &network)?;
                }
                ListingTransport::Authorized => {
                    self.gh_stream(&request, cancel, &mut bytes, &mut |_| {}, &network)?;
                }
            }
            check_cancel(cancel)?;
            let releases: Vec<Release> =
                serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
            if releases.len() > 100 {
                return Err(Error::Limit);
            }
            let count = releases.len();
            for release in releases {
                if release.id == 0 || !ids.insert(release.id) {
                    return Err(Error::Invalid);
                }
                if channel.accepts(&release.tag_name, release.prerelease, release.draft) {
                    matching.push(release);
                }
            }
            if count < 100 {
                break;
            }
            if page == 10 {
                return Err(Error::Limit);
            }
        }
        Ok(matching)
    }
    fn verify_release(
        &self,
        release: &Release,
        channel: Channel,
        cancel: &AtomicBool,
        deadline: Instant,
    ) -> Result<Candidate, Error> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for asset in &release.assets {
            if asset.id == 0 || !ids.insert(asset.id) || !names.insert(&asset.name) {
                return Err(Error::Invalid);
            }
        }
        let find = |name: &str| {
            release
                .assets
                .iter()
                .find(|a| a.name == name)
                .ok_or(Error::Invalid)
        };
        let metadata = find("guild-update.json")?;
        let signature = find("guild-update.json.sig")?;
        if metadata.size > guild_session::update::MAX_MANIFEST as u64 || signature.size > 4096 {
            return Err(Error::Invalid);
        }
        let cached = self
            .verified
            .lock()
            .map_err(|_| Error::Network)?
            .get(release);
        let (bytes, sig) = match cached {
            Some(pair) => pair,
            None => (
                self.asset(
                    metadata.id,
                    guild_session::update::MAX_MANIFEST,
                    cancel,
                    deadline,
                )?,
                self.asset(signature.id, 4096, cancel, deadline)?,
            ),
        };
        check_cancel(cancel)?;
        let manifest = guild_session::update::verify_manifest(&bytes, &sig, &self.keys)
            .map_err(|_| Error::Invalid)?;
        manifest
            .validate(&self.repository, &release.tag_name, channel)
            .map_err(|_| Error::Invalid)?;
        let archive = find(&manifest.archive)?;
        let signature = find(&format!("{}.sig", manifest.archive))?;
        if archive.size != manifest.bytes || signature.size > 4096 {
            return Err(Error::Invalid);
        }
        self.verified
            .lock()
            .map_err(|_| Error::Network)?
            .insert(release, &bytes, &sig);
        Ok(Candidate {
            release_id: release.id,
            archive_id: archive.id,
            signature_id: signature.id,
            manifest,
            manifest_bytes: bytes,
            manifest_signature: sig,
        })
    }
    fn asset(
        &self,
        id: u64,
        limit: usize,
        cancel: &AtomicBool,
        deadline: Instant,
    ) -> Result<Vec<u8>, Error> {
        self.metadata(
            &format!("/repos/{}/releases/assets/{id}", self.repository),
            limit,
            true,
            cancel,
            deadline,
        )
    }
    fn metadata(
        &self,
        path: &str,
        limit: usize,
        asset: bool,
        cancel: &AtomicBool,
        deadline: Instant,
    ) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        let request = Request {
            path,
            limit: limit as u64,
            asset,
            timeout: Duration::from_secs(15).min(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or(Error::Network)?,
            ),
        };
        self.fetch(&request, cancel, &mut bytes, &mut |_| {})?;
        Ok(bytes)
    }
    fn fetch(
        &self,
        request: &Request<'_>,
        cancel: &AtomicBool,
        output: &mut dyn Restartable,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, Error> {
        let network = crate::network::snapshot();
        let deadline = Instant::now() + request.timeout;
        match self.http_stream(request, cancel, output, progress, &network) {
            Err(anonymous @ (Error::Access | Error::RateLimited(_))) => {
                let remaining = Request {
                    timeout: deadline
                        .checked_duration_since(Instant::now())
                        .ok_or(Error::Network)?,
                    ..*request
                };
                let result = self.gh_stream(&remaining, cancel, output, progress, &network);
                match result {
                    Ok(count) => Ok(count),
                    Err(error @ (Error::Access | Error::MissingGh)) => {
                        // An anonymous throttle cannot improve by immediately repeating it.
                        if matches!(anonymous, Error::RateLimited(_)) {
                            return Err(anonymous);
                        }
                        output.restart().map_err(|_| Error::Storage)?;
                        progress(0);
                        let remaining = Request {
                            timeout: deadline
                                .checked_duration_since(Instant::now())
                                .ok_or(Error::Network)?,
                            ..*request
                        };
                        match self.http_stream(&remaining, cancel, output, progress, &network) {
                            Err(Error::Access) => Err(error),
                            other => other,
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            other => other,
        }
    }
    fn gh_stream(
        &self,
        request: &Request<'_>,
        cancel: &AtomicBool,
        output: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
        network: &guild_session::network::Network,
    ) -> Result<u64, Error> {
        #[cfg(test)]
        let override_path = self.gh_override.as_deref();
        #[cfg(not(test))]
        let override_path = None;
        gh::request(request, cancel, output, progress, override_path, network)
    }
    fn http_stream(
        &self,
        request: &Request<'_>,
        cancel: &AtomicBool,
        output: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
        network: &guild_session::network::Network,
    ) -> Result<u64, Error> {
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            self.http_stream_unlogged(request, cancel, output, progress, network)
        })
    }
    fn http_stream_unlogged(
        &self,
        request: &Request<'_>,
        cancel: &AtomicBool,
        output: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
        network: &guild_session::network::Network,
    ) -> Result<u64, Error> {
        let Request {
            path,
            limit,
            asset,
            timeout,
        } = *request;
        check_cancel(cancel)?;
        let deadline = Instant::now() + timeout;
        let mut address =
            Url::parse(&format!("{}{path}", self.origin)).map_err(|_| Error::Invalid)?;
        for redirects in 0..=5 {
            check_cancel(cancel)?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Network)?;
            let agent = idle::agent(remaining, network)?;
            if !self.allowed(&address) {
                return Err(Error::Invalid);
            }
            let mut response = agent
                .get(address.as_str())
                .header(
                    "Accept",
                    if asset {
                        "application/octet-stream"
                    } else {
                        "application/vnd.github+json"
                    },
                )
                .header("X-GitHub-Api-Version", "2026-03-10")
                .header("User-Agent", "Guild-Updater")
                .call()
                .map_err(|_| {
                    if cancel.load(Ordering::Acquire) {
                        Error::Cancelled
                    } else {
                        Error::Network
                    }
                })?;
            let status = response.status().as_u16();
            let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok());
            if let Some(error) = rate_limit(
                status,
                header("x-ratelimit-remaining"),
                header("x-ratelimit-reset"),
                header("retry-after"),
            ) {
                return Err(error);
            }
            if [401, 403, 404].contains(&status) {
                return Err(Error::Access);
            }
            if [301, 302, 303, 307, 308].contains(&status) {
                if redirects == 5 {
                    return Err(Error::Limit);
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(Error::Invalid)?;
                address = address.join(location).map_err(|_| Error::Invalid)?;
                continue;
            }
            if status != 200 {
                return Err(Error::Network);
            }
            if response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .is_some_and(|n| n > limit)
            {
                return Err(Error::Limit);
            }
            let mut total = 0u64;
            let mut reader = response.body_mut().as_reader();
            let mut buffer = [0u8; 16384];
            loop {
                check_cancel(cancel)?;
                let n = reader.read(&mut buffer).map_err(|_| {
                    if cancel.load(Ordering::Acquire) {
                        Error::Cancelled
                    } else {
                        Error::Network
                    }
                })?;
                if n == 0 {
                    break;
                }
                total = total.checked_add(n as u64).ok_or(Error::Limit)?;
                if total > limit {
                    return Err(Error::Limit);
                }
                output.write_all(&buffer[..n]).map_err(|_| Error::Storage)?;
                progress(total);
            }
            return Ok(total);
        }
        Err(Error::Limit)
    }
    fn allowed(&self, url: &Url) -> bool {
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return false;
        }
        #[cfg(test)]
        if self.origin.starts_with("http://127.0.0.1:")
            && url.origin().ascii_serialization() == self.origin
        {
            return true;
        }
        url.scheme() == "https"
            && url.port_or_known_default() == Some(443)
            && matches!(
                url.host_str(),
                Some(
                    "api.github.com"
                        | "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                        | "github-releases.githubusercontent.com"
                )
            )
    }
}
fn rate_limit(
    status: u16,
    remaining: Option<&str>,
    reset: Option<&str>,
    retry: Option<&str>,
) -> Option<Error> {
    if status != 429 && !(status == 403 && remaining == Some("0")) {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let reset = reset.and_then(|s| s.parse::<u64>().ok());
    let retry = retry.and_then(|value| {
        if let Ok(seconds) = value.parse::<u64>() {
            return Some(now.saturating_add(seconds));
        }
        let format = time::macros::format_description!(
            "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT"
        );
        time::PrimitiveDateTime::parse(value, format)
            .ok()?
            .assume_utc()
            .unix_timestamp()
            .try_into()
            .ok()
    });
    Some(Error::RateLimited(
        reset
            .into_iter()
            .chain(retry)
            .max()
            .unwrap_or(now + 1800)
            .max(now + 1),
    ))
}
#[derive(Clone, Copy)]
struct Request<'a> {
    path: &'a str,
    limit: u64,
    asset: bool,
    timeout: Duration,
}
trait Restartable: Write {
    fn restart(&mut self) -> std::io::Result<()>;
}
impl Restartable for Vec<u8> {
    fn restart(&mut self) -> std::io::Result<()> {
        self.clear();
        Ok(())
    }
}
impl Restartable for std::fs::File {
    fn restart(&mut self) -> std::io::Result<()> {
        self.set_len(0)?;
        self.seek(SeekFrom::Start(0))?;
        Ok(())
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), Error> {
    if cancel.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct Release {
    id: u64,
    tag_name: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<Asset>,
    updated_at: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
struct Asset {
    id: u64,
    name: String,
    size: u64,
    updated_at: Option<String>,
    digest: Option<String>,
}

#[cfg(test)]
pub(crate) mod tests;
