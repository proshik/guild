//! Explicit build and staging commands; none executes CI or publishes a release.
use guild_session::update::{self, BuildInfo, Channel, CiIdentity, Provenance, bundle};
use std::{path::Path, sync::atomic::AtomicBool};

pub fn run(args: &[&str]) -> Option<Result<(), String>> {
    match args {
        ["public-key", key, id] => Some(
            update::publisher::public_key(Path::new(key), id)
                .map(|document| println!("{document}")),
        ),
        ["manifest", app, zip, repository, output] => Some(update::publisher::manifest(
            Path::new(app),
            Path::new(zip),
            repository,
            Path::new(output),
        )),
        ["sign", manifest, zip, key, id, output] => Some(update::publisher::sign(
            Path::new(manifest),
            Path::new(zip),
            Path::new(key),
            id,
            Path::new(output),
        )),
        ["verify", directory, keys] => Some(update::publisher::verify(
            Path::new(directory),
            Path::new(keys),
        )),
        ["keys-check", file] => Some(update::publisher::keys(Path::new(file)).map(|_| ())),
        ["select-tag", requested] => Some(select_tag(requested)),
        [
            "build-info",
            output,
            commit,
            tag,
            order,
            cleanliness,
            built,
            repository,
            run,
            attempt,
        ] => Some(build_info(
            output,
            commit,
            tag,
            order,
            cleanliness,
            built,
            repository,
            run,
            attempt,
        )),
        ["stage", source, destination] => Some(
            bundle::stage(
                Path::new(source),
                Path::new(destination),
                &AtomicBool::new(false),
            )
            .map(|_| ()),
        ),
        _ => None,
    }
}
#[allow(clippy::too_many_arguments)]
fn build_info(
    output: &str,
    commit: &str,
    tag: &str,
    order: &str,
    cleanliness: &str,
    built: &str,
    repository: &str,
    run: &str,
    attempt: &str,
) -> Result<(), String> {
    let mut info = BuildInfo::local(commit.to_owned());
    info.dirty = match cleanliness {
        "clean" => false,
        "dirty" => true,
        _ => return Err("expected clean or dirty".into()),
    };
    info.built = built.parse().map_err(|_| "invalid build timestamp")?;
    info.tag = (tag != "-").then(|| tag.to_owned());
    info.channel = if Channel::Nightly.accepts(tag, true, false) {
        Some(Channel::Nightly)
    } else if Channel::Stable.accepts(tag, false, false) {
        Some(Channel::Stable)
    } else {
        None
    };
    info.revision_order = if order == "-" {
        None
    } else {
        Some(order.parse().map_err(|_| "invalid revision order")?)
    };
    if repository != "-" {
        if !update::repository_valid(repository) {
            return Err("invalid repository identity".into());
        }
        info.provenance = Provenance::GithubActions;
        info.ci = Some(CiIdentity {
            repository: repository.to_owned(),
            run_id: run.parse().map_err(|_| "invalid CI run")?,
            attempt: attempt.parse().map_err(|_| "invalid CI attempt")?,
        });
    } else if run != "-" || attempt != "-" {
        return Err("partial CI identity".into());
    }
    info.validate()?;
    let bytes = serde_json::to_vec_pretty(&info).map_err(|_| "could not encode build identity")?;
    bundle::write_new(Path::new(output), &bytes, false)
}

fn select_tag(requested: &str) -> Result<(), String> {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin()
        .take(128 * 1024 + 1)
        .read_to_string(&mut input)
        .map_err(|_| "could not read tags")?;
    if input.len() > 128 * 1024 {
        return Err("tag list exceeds size limit".into());
    }
    let tags: std::collections::BTreeSet<_> = input.lines().filter(|t| !t.is_empty()).collect();
    let release = |t: &str| {
        Channel::Nightly.accepts(t, true, false) || Channel::Stable.accepts(t, false, false)
    };
    if requested != "-" {
        if !tags.contains(requested) || !(release(requested) || requested.starts_with("dogfood-")) {
            return Err("selected tag is invalid or does not name this exact commit".into());
        }
        println!("{requested}");
        return Ok(());
    }
    let releases: Vec<_> = tags.iter().copied().filter(|t| release(t)).collect();
    let choices: Vec<_> = if releases.is_empty() {
        tags.iter()
            .copied()
            .filter(|t| t.starts_with("dogfood-"))
            .collect()
    } else {
        releases
    };
    match choices.as_slice() {
        [] => println!("-"),
        [tag] => println!("{tag}"),
        _ => return Err("multiple build tags: set RELEASE_TAG explicitly".into()),
    }
    Ok(())
}
